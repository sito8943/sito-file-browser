//! End-to-end crypto of the remote channel. The full wire spec (what the Android app must
//! implement byte for byte) is in plans/REMOTE_CONTROL_PLAN.md, section "Protocolo E2E v1".
//!
//! Handshake, Noise-KK style, all on P-256:
//!
//! ```text
//! IKM  = DH(eD, eM) ‖ DH(eD, sM) ‖ DH(sD, eM) ‖ DH(sD, sM) [‖ pairing code]
//! salt = SHA-256(LABEL ‖ mode ‖ len(sid) ‖ sid ‖ sD ‖ sM ‖ eD ‖ eM)      (points, 65 bytes each)
//! keys = HKDF-SHA256(salt, IKM) → "m2d" (32), "d2m" (32), "sas" (4)
//! ```
//!
//! Only the holders of both paired static keys derive the keys (mutual authentication); the
//! ephemeral terms give forward secrecy; the mixed terms keep one leaked static key from
//! letting an attacker impersonate the *other* side. While pairing, the one-time QR code is
//! mixed in as a pre-shared key, so the relay cannot pair a device of its own.
//!
//! Messages: AES-256-GCM, one key per direction, nonce = 4 zero bytes ‖ seq (u64 BE), AAD binds
//! label, direction, session id and seq. Sequence numbers must arrive exactly in order (the relay
//! preserves order on a socket); anything else is a replay or a drop and kills the session.
//!
//! **Pending:** independent cryptographic review before release (plan, decision 4).

use aws_lc_rs::encoding::{AsDer, EcPrivateKeyRfc5915Der};
use aws_lc_rs::{aead, agreement, digest, hkdf};
use zeroize::Zeroize;

pub const LABEL: &[u8] = b"sito-remote/v1";
// Uncompressed SEC1 P-256 point: 0x04 ‖ X ‖ Y.
pub const POINT_LEN: usize = 65;
pub const TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;
const SHARED_LEN: usize = 32;
const SAS_LEN: usize = 4;
const SAS_MODULUS: u32 = 1_000_000;
const UNCOMPRESSED_TAG: u8 = 0x04;

const INFO_M2D: &[u8] = b"sito-remote/v1 m2d";
const INFO_D2M: &[u8] = b"sito-remote/v1 d2m";
const INFO_SAS: &[u8] = b"sito-remote/v1 sas";
// Sealed by the desktop as its first message (seq 0): proves it derived the same keys.
pub const CONFIRM: &[u8] = b"sito-remote/v1 confirm";

// DER prefix of a P-256 SubjectPublicKeyInfo (id-ecPublicKey + prime256v1 + BIT STRING header).
// An SPKI for this curve is always exactly this prefix followed by the 65-byte point, which is
// the format sito-auth stores and Android's `PublicKey.getEncoded()` returns.
const SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];
pub const SPKI_LEN: usize = SPKI_PREFIX.len() + POINT_LEN;

const DIRECTION_D2M: u8 = 0x01;
const DIRECTION_M2D: u8 = 0x02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    InvalidKey,
    // Wrong key, tampered ciphertext or wrong AAD.
    Decrypt,
    // Sequence number other than the next expected one.
    OutOfOrder,
    // A direction ran out of sequence numbers; start a new session.
    Exhausted,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Desktop,
    Mobile,
}

// Handshake mode, part of the transcript so a session handshake can never be replayed as a
// pairing one or vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Session = 0x01,
    Pairing = 0x02,
}

// A P-256 key pair. Used for the desktop's long-lived static key (private part persisted in the
// OS keyring as RFC 5915 DER) and for per-session ephemeral keys (never persisted).
pub struct KeyPair {
    private: agreement::PrivateKey,
    point: [u8; POINT_LEN],
}

impl KeyPair {
    pub fn generate() -> Result<Self, CryptoError> {
        let private = agreement::PrivateKey::generate(&agreement::ECDH_P256)
            .map_err(|_| CryptoError::Internal)?;
        Self::from_private(private)
    }

    pub fn from_der(der: &[u8]) -> Result<Self, CryptoError> {
        let private = agreement::PrivateKey::from_private_key_der(&agreement::ECDH_P256, der)
            .map_err(|_| CryptoError::InvalidKey)?;
        Self::from_private(private)
    }

    pub fn to_der(&self) -> Result<Vec<u8>, CryptoError> {
        let der: EcPrivateKeyRfc5915Der = self
            .private
            .as_der()
            .map_err(|_| CryptoError::Internal)?;
        Ok(der.as_ref().to_vec())
    }

    pub fn point(&self) -> &[u8; POINT_LEN] {
        &self.point
    }

    pub fn spki(&self) -> Vec<u8> {
        point_to_spki(&self.point)
    }

    fn from_private(private: agreement::PrivateKey) -> Result<Self, CryptoError> {
        let public = private
            .compute_public_key()
            .map_err(|_| CryptoError::Internal)?;
        let point = parse_point(public.as_ref())?;
        Ok(Self { private, point })
    }

    // Raw ECDH: the shared point's X coordinate (what JCA's KeyAgreement("ECDH") returns).
    // aws-lc-rs rejects points not on the curve, so a malicious peer key fails here.
    fn agree(&self, peer: &[u8; POINT_LEN]) -> Result<[u8; SHARED_LEN], CryptoError> {
        agreement::agree(
            &self.private,
            agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, peer),
            CryptoError::InvalidKey,
            |secret| {
                let mut out = [0u8; SHARED_LEN];
                if secret.len() != SHARED_LEN {
                    return Err(CryptoError::InvalidKey);
                }
                out.copy_from_slice(secret);
                Ok(out)
            },
        )
    }
}

pub fn parse_point(bytes: &[u8]) -> Result<[u8; POINT_LEN], CryptoError> {
    if bytes.len() != POINT_LEN || bytes[0] != UNCOMPRESSED_TAG {
        return Err(CryptoError::InvalidKey);
    }
    let mut point = [0u8; POINT_LEN];
    point.copy_from_slice(bytes);
    Ok(point)
}

pub fn point_to_spki(point: &[u8; POINT_LEN]) -> Vec<u8> {
    let mut spki = Vec::with_capacity(SPKI_LEN);
    spki.extend_from_slice(&SPKI_PREFIX);
    spki.extend_from_slice(point);
    spki
}

pub fn spki_to_point(spki: &[u8]) -> Result<[u8; POINT_LEN], CryptoError> {
    if spki.len() != SPKI_LEN || spki[..SPKI_PREFIX.len()] != SPKI_PREFIX {
        return Err(CryptoError::InvalidKey);
    }
    parse_point(&spki[SPKI_PREFIX.len()..])
}

// Everything both sides must agree on. Points are uncompressed (65 bytes).
pub struct Handshake<'a> {
    pub mode: Mode,
    pub sid: &'a str,
    pub desktop_static: [u8; POINT_LEN],
    pub mobile_static: [u8; POINT_LEN],
    pub desktop_ephemeral: [u8; POINT_LEN],
    pub mobile_ephemeral: [u8; POINT_LEN],
    // The one-time pairing code (Mode::Pairing only).
    pub psk: Option<&'a [u8]>,
}

// Derive the session from this side's static + ephemeral private keys.
pub fn derive(
    role: Role,
    my_static: &KeyPair,
    my_ephemeral: &KeyPair,
    handshake: &Handshake,
) -> Result<Channel, CryptoError> {
    let sid_len = u8::try_from(handshake.sid.len()).map_err(|_| CryptoError::InvalidKey)?;
    if (handshake.mode == Mode::Pairing) != handshake.psk.is_some() {
        return Err(CryptoError::InvalidKey);
    }
    let (own_static, own_ephemeral, peer_static, peer_ephemeral) = match role {
        Role::Desktop => (
            &handshake.desktop_static,
            &handshake.desktop_ephemeral,
            &handshake.mobile_static,
            &handshake.mobile_ephemeral,
        ),
        Role::Mobile => (
            &handshake.mobile_static,
            &handshake.mobile_ephemeral,
            &handshake.desktop_static,
            &handshake.desktop_ephemeral,
        ),
    };
    // The caller's keys must be the ones the transcript names, or both sides would silently
    // hash different transcripts.
    if my_static.point() != own_static || my_ephemeral.point() != own_ephemeral {
        return Err(CryptoError::InvalidKey);
    }

    // Fixed order regardless of role: eD·eM, eD·sM, sD·eM, sD·sM.
    let terms = match role {
        Role::Desktop => [
            my_ephemeral.agree(peer_ephemeral)?,
            my_ephemeral.agree(peer_static)?,
            my_static.agree(peer_ephemeral)?,
            my_static.agree(peer_static)?,
        ],
        Role::Mobile => [
            my_ephemeral.agree(peer_ephemeral)?,
            my_static.agree(peer_ephemeral)?,
            my_ephemeral.agree(peer_static)?,
            my_static.agree(peer_static)?,
        ],
    };
    let mut ikm = Vec::with_capacity(SHARED_LEN * terms.len() + KEY_LEN);
    for term in &terms {
        ikm.extend_from_slice(term);
    }
    if let Some(psk) = handshake.psk {
        ikm.extend_from_slice(psk);
    }

    let mut transcript = digest::Context::new(&digest::SHA256);
    transcript.update(LABEL);
    transcript.update(&[handshake.mode as u8, sid_len]);
    transcript.update(handshake.sid.as_bytes());
    transcript.update(&handshake.desktop_static);
    transcript.update(&handshake.mobile_static);
    transcript.update(&handshake.desktop_ephemeral);
    transcript.update(&handshake.mobile_ephemeral);
    let salt = transcript.finish();

    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt.as_ref()).extract(&ikm);
    ikm.zeroize();
    let mut m2d = [0u8; KEY_LEN];
    let mut d2m = [0u8; KEY_LEN];
    let mut sas = [0u8; SAS_LEN];
    expand(&prk, INFO_M2D, &mut m2d)?;
    expand(&prk, INFO_D2M, &mut d2m)?;
    expand(&prk, INFO_SAS, &mut sas)?;

    let (send, recv, send_direction) = match role {
        Role::Desktop => (&d2m, &m2d, DIRECTION_D2M),
        Role::Mobile => (&m2d, &d2m, DIRECTION_M2D),
    };
    let channel = Channel {
        send: aead_key(send)?,
        recv: aead_key(recv)?,
        send_direction,
        sid: handshake.sid.to_string(),
        // m2d seq 0 is the plaintext hello, so mobile data starts at 1; the desktop's first
        // sealed message (seq 0) is CONFIRM inside its ack.
        next_send: if role == Role::Desktop { 0 } else { 1 },
        next_recv: if role == Role::Desktop { 1 } else { 0 },
        sas: u32::from_be_bytes(sas) % SAS_MODULUS,
    };
    m2d.zeroize();
    d2m.zeroize();
    Ok(channel)
}

// An established, directional AES-256-GCM channel.
pub struct Channel {
    send: aead::LessSafeKey,
    recv: aead::LessSafeKey,
    send_direction: u8,
    sid: String,
    next_send: u64,
    next_recv: u64,
    sas: u32,
}

impl Channel {
    // 6-digit short authentication string shown on both screens while pairing.
    pub fn sas(&self) -> String {
        format!("{:06}", self.sas)
    }

    pub fn sid(&self) -> &str {
        &self.sid
    }

    // Encrypt the next outgoing message. Returns its seq (the relay envelope's `seq`).
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<(u64, Vec<u8>), CryptoError> {
        let seq = self.next_send;
        if seq == u64::MAX {
            return Err(CryptoError::Exhausted);
        }
        let mut buffer = plaintext.to_vec();
        self.send
            .seal_in_place_append_tag(
                nonce(seq),
                aead::Aad::from(aad(self.send_direction, &self.sid, seq)),
                &mut buffer,
            )
            .map_err(|_| CryptoError::Internal)?;
        self.next_send += 1;
        Ok((seq, buffer))
    }

    // Decrypt the next incoming message. `seq` must be exactly the next expected one.
    pub fn open(&mut self, seq: u64, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if seq != self.next_recv {
            return Err(CryptoError::OutOfOrder);
        }
        if seq == u64::MAX {
            return Err(CryptoError::Exhausted);
        }
        let direction = if self.send_direction == DIRECTION_D2M {
            DIRECTION_M2D
        } else {
            DIRECTION_D2M
        };
        let mut buffer = ciphertext.to_vec();
        let plaintext_len = self
            .recv
            .open_in_place(
                nonce(seq),
                aead::Aad::from(aad(direction, &self.sid, seq)),
                &mut buffer,
            )
            .map_err(|_| CryptoError::Decrypt)?
            .len();
        buffer.truncate(plaintext_len);
        self.next_recv += 1;
        Ok(buffer)
    }
}

struct OutputLen(usize);

impl hkdf::KeyType for OutputLen {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(prk: &hkdf::Prk, info: &[u8], out: &mut [u8]) -> Result<(), CryptoError> {
    let info = [info];
    prk.expand(&info, OutputLen(out.len()))
        .and_then(|okm| okm.fill(out))
        .map_err(|_| CryptoError::Internal)
}

fn aead_key(bytes: &[u8; KEY_LEN]) -> Result<aead::LessSafeKey, CryptoError> {
    aead::UnboundKey::new(&aead::AES_256_GCM, bytes)
        .map(aead::LessSafeKey::new)
        .map_err(|_| CryptoError::Internal)
}

fn nonce(seq: u64) -> aead::Nonce {
    let mut bytes = [0u8; aead::NONCE_LEN];
    bytes[4..].copy_from_slice(&seq.to_be_bytes());
    aead::Nonce::assume_unique_for_key(bytes)
}

// LABEL ‖ direction ‖ len(sid) ‖ sid ‖ seq (u64 BE). The sid length always fits a u8 (derive()
// refuses longer ids).
fn aad(direction: u8, sid: &str, seq: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(LABEL.len() + 2 + sid.len() + 8);
    aad.extend_from_slice(LABEL);
    aad.push(direction);
    aad.push(sid.len() as u8);
    aad.extend_from_slice(sid.as_bytes());
    aad.extend_from_slice(&seq.to_be_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::encoding::PublicKeyX509Der;

    struct Pair {
        desktop: Channel,
        mobile: Channel,
    }

    fn params<'a>(
        mode: Mode,
        keys: [&KeyPair; 4],
        psk: Option<&'a [u8]>,
    ) -> Handshake<'a> {
        let [ds, ms, de, me] = keys;
        Handshake {
            mode,
            sid: "0123456789abcdef",
            desktop_static: *ds.point(),
            mobile_static: *ms.point(),
            desktop_ephemeral: *de.point(),
            mobile_ephemeral: *me.point(),
            psk,
        }
    }

    fn handshake(mode: Mode, desktop_psk: Option<&[u8]>, mobile_psk: Option<&[u8]>) -> Pair {
        let (ds, de) = (KeyPair::generate().unwrap(), KeyPair::generate().unwrap());
        let (ms, me) = (KeyPair::generate().unwrap(), KeyPair::generate().unwrap());
        let keys = [&ds, &ms, &de, &me];
        let desktop = params(mode, keys, desktop_psk);
        let mobile = params(mode, keys, mobile_psk);
        Pair {
            desktop: derive(Role::Desktop, &ds, &de, &desktop).unwrap(),
            mobile: derive(Role::Mobile, &ms, &me, &mobile).unwrap(),
        }
    }

    #[test]
    fn spki_prefix_matches_aws_lc_encoding() {
        let pair = KeyPair::generate().unwrap();
        let der: PublicKeyX509Der = pair.private.compute_public_key().unwrap().as_der().unwrap();
        assert_eq!(der.as_ref(), pair.spki().as_slice());
        assert_eq!(spki_to_point(&pair.spki()).unwrap(), *pair.point());
    }

    #[test]
    fn private_key_der_round_trip() {
        let pair = KeyPair::generate().unwrap();
        let restored = KeyPair::from_der(&pair.to_der().unwrap()).unwrap();
        assert_eq!(restored.point(), pair.point());
    }

    #[test]
    fn both_sides_derive_the_same_channel() {
        let mut pair = handshake(Mode::Session, None, None);
        assert_eq!(pair.desktop.sas(), pair.mobile.sas());

        // Desktop confirm at d2m seq 0.
        let (seq, sealed) = pair.desktop.seal(CONFIRM).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(pair.mobile.open(seq, &sealed).unwrap(), CONFIRM);

        // Mobile data starts at m2d seq 1 (seq 0 was the plaintext hello).
        let (seq, sealed) = pair.mobile.seal(b"{\"op\":\"roots.list\"}").unwrap();
        assert_eq!(seq, 1);
        assert_eq!(
            pair.desktop.open(seq, &sealed).unwrap(),
            b"{\"op\":\"roots.list\"}"
        );
    }

    #[test]
    fn rejects_replay_reorder_and_tampering() {
        let mut pair = handshake(Mode::Session, None, None);
        let (seq1, first) = pair.mobile.seal(b"one").unwrap();
        let (seq2, second) = pair.mobile.seal(b"two").unwrap();
        assert_eq!(
            pair.desktop.open(seq2, &second),
            Err(CryptoError::OutOfOrder)
        );
        assert_eq!(pair.desktop.open(seq1, &first).unwrap(), b"one");
        assert_eq!(pair.desktop.open(seq1, &first), Err(CryptoError::OutOfOrder));

        let mut tampered = second.clone();
        tampered[0] ^= 1;
        assert_eq!(pair.desktop.open(seq2, &tampered), Err(CryptoError::Decrypt));
        // A message cannot be reflected back to its sender.
        let (seq, sealed) = pair.desktop.seal(b"x").unwrap();
        assert_eq!(pair.desktop.open(seq, &sealed), Err(CryptoError::OutOfOrder));
    }

    #[test]
    fn pairing_needs_the_same_code() {
        let code = [7u8; 16];
        let mut good = handshake(Mode::Pairing, Some(&code), Some(&code));
        let (seq, sealed) = good.desktop.seal(CONFIRM).unwrap();
        assert!(good.mobile.open(seq, &sealed).is_ok());

        let mut bad = handshake(Mode::Pairing, Some(&code), Some(&[8u8; 16]));
        let (seq, sealed) = bad.desktop.seal(CONFIRM).unwrap();
        assert_eq!(bad.mobile.open(seq, &sealed), Err(CryptoError::Decrypt));
    }

    #[test]
    fn rejects_bad_keys_and_mode_mismatch() {
        assert!(parse_point(&[0x04; 64]).is_err());
        assert!(parse_point(&[0x02; 65]).is_err());
        assert!(spki_to_point(&[0u8; SPKI_LEN]).is_err());

        let (s, e) = (KeyPair::generate().unwrap(), KeyPair::generate().unwrap());
        let mut off_curve = [0u8; POINT_LEN];
        off_curve[0] = UNCOMPRESSED_TAG;
        let base = Handshake {
            mode: Mode::Session,
            sid: "s",
            desktop_static: *s.point(),
            mobile_static: off_curve,
            desktop_ephemeral: *e.point(),
            mobile_ephemeral: off_curve,
            psk: None,
        };
        assert_eq!(
            derive(Role::Desktop, &s, &e, &base).err(),
            Some(CryptoError::InvalidKey)
        );
        let pairing_without_code = Handshake {
            mode: Mode::Pairing,
            ..base
        };
        assert_eq!(
            derive(Role::Desktop, &s, &e, &pairing_without_code).err(),
            Some(CryptoError::InvalidKey)
        );
    }
}
