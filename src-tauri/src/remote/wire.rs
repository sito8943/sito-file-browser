//! Binary frames carried in the relay envelope's `p` (base64). Binary instead of JSON so a file
//! chunk is base64-encoded once on the wire, which keeps a 256 KB chunk under the relay's 512 KiB
//! frame cap. Layouts (spec: plans/REMOTE_CONTROL_PLAN.md, "Protocolo E2E v1"):
//!
//! ```text
//! HELLO       0x01 ‖ version ‖ mobile ephemeral point (65)                    m→d, seq 0
//! PAIR_HELLO  0x02 ‖ version ‖ mobile ephemeral point (65) ‖ mobile SPKI (91)  m→d, seq 0
//! DATA        0x03 ‖ AES-GCM ciphertext+tag                                    both
//! HELLO_ACK   0x11 ‖ desktop ephemeral point (65) ‖ sealed CONFIRM             d→m, seq 0
//! PAIR_ACK    0x12 ‖ desktop ephemeral point (65) ‖ sealed CONFIRM             d→m, seq 0
//! REJECT      0x7F ‖ code                                                      d→m, plaintext
//! ```
//!
//! REJECT is unauthenticated (it is sent precisely when no channel exists), so the phone may only
//! use it as a hint for what to show, never as proof of anything.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use super::crypto::{parse_point, CONFIRM, POINT_LEN, SPKI_LEN, TAG_LEN};

pub const WIRE_VERSION: u8 = 1;

const KIND_HELLO: u8 = 0x01;
const KIND_PAIR_HELLO: u8 = 0x02;
const KIND_DATA: u8 = 0x03;
const KIND_HELLO_ACK: u8 = 0x11;
const KIND_PAIR_ACK: u8 = 0x12;
const KIND_REJECT: u8 = 0x7F;

const CONFIRM_SEALED_LEN: usize = CONFIRM.len() + TAG_LEN;

// Why the desktop refused a hello.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RejectCode {
    // The sender's device id is not in the trusted list (never paired or revoked).
    NotPaired = 1,
    // PAIR_HELLO without an open pairing window (no QR shown, expired, or code already used).
    PairingClosed = 2,
    // The registry entry does not match: unknown device, not a phone, or a different key.
    DeviceMismatch = 3,
    UnsupportedVersion = 4,
    // Remote is shutting down or another session is being set up.
    Busy = 5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireFrame {
    Hello {
        ephemeral: [u8; POINT_LEN],
    },
    PairHello {
        ephemeral: [u8; POINT_LEN],
        static_spki: Vec<u8>,
    },
    Data {
        ciphertext: Vec<u8>,
    },
    HelloAck {
        ephemeral: [u8; POINT_LEN],
        confirm: Vec<u8>,
    },
    PairAck {
        ephemeral: [u8; POINT_LEN],
        confirm: Vec<u8>,
    },
    // Raw byte: codes added by newer desktops still decode on older phones.
    Reject {
        code: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    Malformed,
    UnsupportedVersion,
}

pub fn encode(frame: &WireFrame) -> String {
    let mut bytes = Vec::new();
    match frame {
        WireFrame::Hello { ephemeral } => {
            bytes.extend_from_slice(&[KIND_HELLO, WIRE_VERSION]);
            bytes.extend_from_slice(ephemeral);
        }
        WireFrame::PairHello {
            ephemeral,
            static_spki,
        } => {
            bytes.extend_from_slice(&[KIND_PAIR_HELLO, WIRE_VERSION]);
            bytes.extend_from_slice(ephemeral);
            bytes.extend_from_slice(static_spki);
        }
        WireFrame::Data { ciphertext } => {
            bytes.push(KIND_DATA);
            bytes.extend_from_slice(ciphertext);
        }
        WireFrame::HelloAck { ephemeral, confirm } => {
            bytes.push(KIND_HELLO_ACK);
            bytes.extend_from_slice(ephemeral);
            bytes.extend_from_slice(confirm);
        }
        WireFrame::PairAck { ephemeral, confirm } => {
            bytes.push(KIND_PAIR_ACK);
            bytes.extend_from_slice(ephemeral);
            bytes.extend_from_slice(confirm);
        }
        WireFrame::Reject { code } => {
            bytes.extend_from_slice(&[KIND_REJECT, *code]);
        }
    }
    BASE64.encode(bytes)
}

pub fn decode(payload: &str) -> Result<WireFrame, WireError> {
    let bytes = BASE64.decode(payload).map_err(|_| WireError::Malformed)?;
    let (&kind, rest) = bytes.split_first().ok_or(WireError::Malformed)?;
    match kind {
        KIND_HELLO | KIND_PAIR_HELLO => {
            let (&version, rest) = rest.split_first().ok_or(WireError::Malformed)?;
            if version != WIRE_VERSION {
                return Err(WireError::UnsupportedVersion);
            }
            if rest.len() < POINT_LEN {
                return Err(WireError::Malformed);
            }
            let (point, tail) = rest.split_at(POINT_LEN);
            let ephemeral = parse_point(point).map_err(|_| WireError::Malformed)?;
            match (kind, tail.len()) {
                (KIND_HELLO, 0) => Ok(WireFrame::Hello { ephemeral }),
                (KIND_PAIR_HELLO, SPKI_LEN) => Ok(WireFrame::PairHello {
                    ephemeral,
                    static_spki: tail.to_vec(),
                }),
                _ => Err(WireError::Malformed),
            }
        }
        KIND_DATA if rest.len() >= TAG_LEN => Ok(WireFrame::Data {
            ciphertext: rest.to_vec(),
        }),
        KIND_HELLO_ACK | KIND_PAIR_ACK if rest.len() == POINT_LEN + CONFIRM_SEALED_LEN => {
            let (point, confirm) = rest.split_at(POINT_LEN);
            let ephemeral = parse_point(point).map_err(|_| WireError::Malformed)?;
            let confirm = confirm.to_vec();
            Ok(if kind == KIND_HELLO_ACK {
                WireFrame::HelloAck { ephemeral, confirm }
            } else {
                WireFrame::PairAck { ephemeral, confirm }
            })
        }
        KIND_REJECT if rest.len() == 1 => Ok(WireFrame::Reject { code: rest[0] }),
        _ => Err(WireError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(fill: u8) -> [u8; POINT_LEN] {
        let mut point = [fill; POINT_LEN];
        point[0] = 0x04;
        point
    }

    #[test]
    fn round_trips_every_frame() {
        let frames = [
            WireFrame::Hello { ephemeral: point(1) },
            WireFrame::PairHello {
                ephemeral: point(2),
                static_spki: vec![9; SPKI_LEN],
            },
            WireFrame::Data {
                ciphertext: vec![3; TAG_LEN + 5],
            },
            WireFrame::HelloAck {
                ephemeral: point(4),
                confirm: vec![5; CONFIRM_SEALED_LEN],
            },
            WireFrame::PairAck {
                ephemeral: point(6),
                confirm: vec![7; CONFIRM_SEALED_LEN],
            },
            WireFrame::Reject {
                code: RejectCode::NotPaired as u8,
            },
        ];
        for frame in frames {
            assert_eq!(decode(&encode(&frame)), Ok(frame.clone()), "{frame:?}");
        }
    }

    #[test]
    fn rejects_malformed_frames() {
        let hello_v2 = BASE64.encode([&[KIND_HELLO, 2][..], &point(1)[..]].concat());
        assert_eq!(decode(&hello_v2), Err(WireError::UnsupportedVersion));
        for bad in [
            String::new(),
            "not base64!".to_string(),
            BASE64.encode([KIND_HELLO, WIRE_VERSION]),
            BASE64.encode([KIND_DATA, 1, 2]),
            BASE64.encode([KIND_REJECT]),
            BASE64.encode([0x42, 0]),
            // HELLO with trailing bytes.
            BASE64.encode([&[KIND_HELLO, WIRE_VERSION][..], &point(1)[..], &[0][..]].concat()),
            // Compressed point instead of uncompressed.
            BASE64.encode([&[KIND_HELLO, WIRE_VERSION, 0x02][..], &[1; 64][..]].concat()),
        ] {
            assert_eq!(decode(&bad), Err(WireError::Malformed), "{bad:?}");
        }
    }
}
