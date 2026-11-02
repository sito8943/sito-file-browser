//! Pairing state (security rule 4: explicit per-device pairing, nothing plug-and-play).
//!
//! 1. The user opens "Pair device"; `start` creates a one-time 128-bit code valid for 2 minutes
//!    and returns the QR payload.
//! 2. The phone scans it and sends PAIR_HELLO. The session layer takes the code (`take_code`:
//!    single use, whatever the outcome) and runs the handshake with it as pre-shared key. A wrong
//!    or stale code simply yields different keys, so the code is never compared directly.
//! 3. Once the phone's first sealed message decrypts (proof it knows the code and holds its static
//!    key), the device becomes `pending` and the phone shows the 6-digit SAS.
//! 4. The user types that SAS on the desktop. Only a match persists the device in remote.toml
//!    (`take_pending`, one attempt: a wrong code rejects the phone). Cancelling or a new `start`
//!    discards everything.

use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Serialize;

use super::relay_client::RELAY_URL;

pub const CODE_LEN: usize = 16;
pub const PAIRING_TTL: Duration = Duration::from_secs(120);
const QR_VERSION: u8 = 1;

const ERROR_RANDOM: &str = "REMOTE_PAIRING_RANDOM_FAILED";

// QR payload, serialized as compact JSON. Short keys keep the QR at a low density.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PairingQr {
    // Payload version.
    pub v: u8,
    // Desktop sito-auth device id.
    pub d: String,
    // Desktop static public key, SPKI DER base64 (same encoding as the sito-auth registry).
    pub k: String,
    // One-time code, base64url without padding.
    pub c: String,
    // Relay URL.
    pub r: String,
}

impl PairingQr {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// A phone that proved the code and is waiting for the user to type the SAS it shows.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingDevice {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    // SPKI DER base64.
    pub public_key: String,
    // Never sent to the UI: the desktop does not display it, the user types what the phone shows
    // and `sas_matches` checks it here. Showing it on both screens invited a blind "match" click.
    #[serde(skip_serializing)]
    pub sas: String,
}

impl PendingDevice {
    // Compare what the user typed (spaces and separators ignored) with the derived SAS.
    pub fn sas_matches(&self, typed: &str) -> bool {
        let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
        aws_lc_rs::constant_time::verify_slices_are_equal(digits.as_bytes(), self.sas.as_bytes())
            .is_ok()
    }
}

struct Offer {
    code: [u8; CODE_LEN],
    expires: Instant,
}

#[derive(Default)]
pub struct Pairing {
    offer: Option<Offer>,
    pending: Option<PendingDevice>,
}

impl Pairing {
    // Open a pairing window. Replaces any previous code and pending device.
    pub fn start(
        &mut self,
        desktop_device_id: &str,
        desktop_spki: &[u8],
        now: Instant,
    ) -> Result<PairingQr, String> {
        let mut code = [0u8; CODE_LEN];
        aws_lc_rs::rand::fill(&mut code).map_err(|_| ERROR_RANDOM.to_string())?;
        self.offer = Some(Offer {
            code,
            expires: now + PAIRING_TTL,
        });
        self.pending = None;
        Ok(PairingQr {
            v: QR_VERSION,
            d: desktop_device_id.to_string(),
            k: BASE64.encode(desktop_spki),
            c: URL_SAFE_NO_PAD.encode(code),
            r: RELAY_URL.to_string(),
        })
    }

    // Consume the code for one PAIR_HELLO. None when no window is open or it expired. Single use
    // even if the handshake then fails, so the code cannot be brute-forced online.
    pub fn take_code(&mut self, now: Instant) -> Option<[u8; CODE_LEN]> {
        let offer = self.offer.take()?;
        (now < offer.expires).then_some(offer.code)
    }

    pub fn set_pending(&mut self, device: PendingDevice) {
        self.pending = Some(device);
    }

    pub fn pending(&self) -> Option<&PendingDevice> {
        self.pending.as_ref()
    }

    // The user confirmed the SAS for this device; hand it over for persisting.
    pub fn take_pending(&mut self, device_id: &str) -> Option<PendingDevice> {
        if self.pending.as_ref()?.device_id != device_id {
            return None;
        }
        self.pending.take()
    }

    pub fn cancel(&mut self) {
        self.offer = None;
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(id: &str) -> PendingDevice {
        PendingDevice {
            device_id: id.to_string(),
            name: "Pixel".to_string(),
            platform: "ANDROID".to_string(),
            public_key: "k".to_string(),
            sas: "123456".to_string(),
        }
    }

    #[test]
    fn qr_carries_a_fresh_one_time_code() {
        let mut pairing = Pairing::default();
        let now = Instant::now();
        let first = pairing.start("5", &[1, 2, 3], now).unwrap();
        let second = pairing.start("5", &[1, 2, 3], now).unwrap();
        assert_ne!(first.c, second.c);
        assert_eq!(URL_SAFE_NO_PAD.decode(&second.c).unwrap().len(), CODE_LEN);
        assert_eq!(second.k, "AQID");
        assert_eq!(second.r, RELAY_URL);

        // Only the latest code is live, and only once.
        let code = pairing.take_code(now).unwrap();
        assert_eq!(URL_SAFE_NO_PAD.encode(code), second.c);
        assert!(pairing.take_code(now).is_none());
    }

    #[test]
    fn code_expires() {
        let mut pairing = Pairing::default();
        let now = Instant::now();
        pairing.start("5", &[1], now).unwrap();
        assert!(pairing.take_code(now + PAIRING_TTL).is_none());
    }

    #[test]
    fn pending_device_needs_matching_confirmation() {
        let mut pairing = Pairing::default();
        pairing.set_pending(pending("9"));
        assert!(pairing.take_pending("8").is_none());
        assert_eq!(pairing.take_pending("9"), Some(pending("9")));
        assert!(pairing.pending().is_none());

        pairing.set_pending(pending("9"));
        pairing.cancel();
        assert!(pairing.take_pending("9").is_none());
    }

    #[test]
    fn typed_sas_must_match_exactly() {
        let device = pending("9");
        assert!(device.sas_matches("123456"));
        assert!(device.sas_matches("123 456"));
        assert!(!device.sas_matches("123457"));
        assert!(!device.sas_matches("12345"));
        assert!(!device.sas_matches(""));
        // The UI never receives the SAS.
        let json = serde_json::to_value(&device).unwrap();
        assert!(json.get("sas").is_none());
    }
}
