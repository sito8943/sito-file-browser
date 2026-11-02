//! Remote control (0.18.0): lets a paired Android device browse user-authorized folders and copy
//! files to the phone through the sito-remote-relay. Design and the fixed security rules live in
//! `plans/REMOTE_CONTROL_PLAN.md`; every piece here must respect them:
//!
//! 1. read-only + copy-to-phone only;
//! 2. access limited to authorized roots, never the whole disk;
//! 3. disabled by default (no relay connection while the setting is off);
//! 4. explicit per-device pairing (QR + SAS), revocable from the desktop;
//! 5. end-to-end encryption (the relay only sees ciphertext).
//!
//! The desktop is the real security boundary: even with a stolen sito-auth account or a compromised
//! relay, it only serves sessions from public keys it paired itself and only paths inside roots.

pub mod auth;
pub mod commands;
pub mod crypto;
pub mod error;
pub mod handlers;
pub mod pairing;
pub mod paths;
pub mod relay_client;
pub mod secrets;
pub mod service;
pub mod session;
pub mod store;
pub mod volume;
pub mod wire;
