// OS credential store for remote-control secrets: the sito-auth refresh token and this desktop's
// P-256 private key. macOS Keychain, Windows Credential Manager, Linux Secret Service. Separate
// service from SFTP (filesystem/sftp.rs) so revoking one never touches the other.
//
// Every call can block (and on macOS may show a Keychain prompt), so callers run these on the
// blocking pool, never on Tauri's main thread or inside the relay socket task.
//
// Unlike the SFTP keychain helpers these fail closed: there is no plaintext fallback, and a store
// error is reported instead of being read as "no secret".

const SERVICE: &str = "sito-file-browser-remote";

pub const REFRESH_TOKEN: &str = "refresh-token";
pub const DEVICE_PRIVATE_KEY: &str = "device-private-key";

const ERROR_UNAVAILABLE: &str = "REMOTE_KEYRING_UNAVAILABLE";

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod imp {
    use keyring::{Entry, Error};

    fn entry(name: &str) -> Result<Entry, String> {
        Entry::new(super::SERVICE, name).map_err(|_| super::ERROR_UNAVAILABLE.to_string())
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        match entry(name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(Error::NoEntry) => Ok(None),
            Err(_) => Err(super::ERROR_UNAVAILABLE.to_string()),
        }
    }

    pub fn set(name: &str, secret: &str) -> Result<(), String> {
        entry(name)?
            .set_password(secret)
            .map_err(|_| super::ERROR_UNAVAILABLE.to_string())
    }

    // A missing entry already is the desired end state.
    pub fn delete(name: &str) -> Result<(), String> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(_) => Err(super::ERROR_UNAVAILABLE.to_string()),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    pub fn get(_name: &str) -> Result<Option<String>, String> {
        Err(super::ERROR_UNAVAILABLE.to_string())
    }
    pub fn set(_name: &str, _secret: &str) -> Result<(), String> {
        Err(super::ERROR_UNAVAILABLE.to_string())
    }
    pub fn delete(_name: &str) -> Result<(), String> {
        Err(super::ERROR_UNAVAILABLE.to_string())
    }
}

pub use imp::{delete, get, set};
