use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::functions::cleanup::{PROTECTED_EXACT, PROTECTED_PREFIXES};

use super::volume::is_network_path;

// remote.toml lives directly in the app config dir, beside settings.toml and cleanup.toml. It holds
// nothing secret: the device private key and the sito-auth refresh token live in the OS keyring.
const REMOTE_FILE: &str = "remote.toml";

// Stable error codes the frontend maps to localized copy (same convention as cleanup.rs).
const ERROR_NOT_ABSOLUTE: &str = "REMOTE_ROOT_NOT_ABSOLUTE";
const ERROR_NOT_A_DIR: &str = "REMOTE_ROOT_NOT_A_DIR";
const ERROR_PROTECTED: &str = "REMOTE_ROOT_PROTECTED";
const ERROR_NETWORK: &str = "REMOTE_ROOT_NETWORK";
const ERROR_DUPLICATE: &str = "REMOTE_ROOT_DUPLICATE";
const ERROR_UNKNOWN_ROOT: &str = "REMOTE_UNKNOWN_ROOT";
const ERROR_DEVICE_DUPLICATE: &str = "REMOTE_DEVICE_DUPLICATE";
const ERROR_UNKNOWN_DEVICE: &str = "REMOTE_UNKNOWN_DEVICE";

// A folder the user authorized for remote browsing. The phone only ever sees `id` and `name`;
// `path` never leaves the desktop (requests carry root id + a relative path, see remote/paths.rs).
// `path` is stored canonicalized so duplicates collapse and paths.rs can detect a root that was
// swapped for a symlink after being authorized.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RemoteRoot {
    pub id: String,
    pub name: String,
    pub path: String,
}

// A phone the user explicitly paired (QR + SAS confirmed on the desktop). Only sessions whose
// handshake proves possession of `public_key` are served; removing the entry revokes immediately.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    // sito-auth device id (the relay routes by it).
    pub id: String,
    pub name: String,
    pub platform: String,
    // P-256 public key, SPKI DER, base64.
    pub public_key: String,
    // Unix seconds.
    pub paired_at: u64,
}

// remote.toml root. A struct (not bare Vecs) so the file can grow new top-level keys later without
// breaking the format.
#[derive(Debug, Deserialize, Serialize, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    // This desktop's sito-auth device id, set once it registered with POST /devices.
    #[serde(default)]
    pub desktop_device_id: Option<String>,
    #[serde(default)]
    pub roots: Vec<RemoteRoot>,
    #[serde(default)]
    pub devices: Vec<PairedDevice>,
}

impl RemoteConfig {
    pub fn root(&self, id: &str) -> Option<&RemoteRoot> {
        self.roots.iter().find(|root| root.id == id)
    }

    pub fn device(&self, id: &str) -> Option<&PairedDevice> {
        self.devices.iter().find(|device| device.id == id)
    }
}

// Read remote.toml from a given config dir. Absent → empty config. A file that exists but does not
// parse is an error instead of a silent default: writing back a default would wipe the user's
// roots and paired devices.
pub fn read_config_from(config_dir: &Path) -> Result<RemoteConfig, String> {
    match std::fs::read_to_string(config_dir.join(REMOTE_FILE)) {
        Ok(content) => toml::from_str(&content).map_err(|e| e.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RemoteConfig::default()),
        Err(error) => Err(error.to_string()),
    }
}

// Write remote.toml into a given config dir, creating the dir if needed.
pub fn write_config_to(config_dir: &Path, config: &RemoteConfig) -> Result<(), String> {
    let serialized = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join(REMOTE_FILE), serialized).map_err(|e| e.to_string())
}

// Validate a candidate root and return its canonical path. Security rule 2 (never the whole disk):
// rejected are relative paths, non-directories, the filesystem root, volume roots, the home dir,
// OS-owned trees, network filesystems (SMB/NFS/mapped drives: remote must not relay a third
// machine), and anything equal to, inside, or an ancestor of the app config dir (which holds
// remote.toml and settings). Being an ancestor of the config dir also rules out the home dir and
// /Users on every platform where the config dir lives under home.
pub fn guard_root(path: &Path, config_dir: &Path, home: Option<&Path>) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(ERROR_NOT_ABSOLUTE.to_string());
    }

    // canonicalize resolves symlinks and `..` first, so none of the checks below can be bypassed
    // by aliasing a protected directory.
    let resolved = path
        .canonicalize()
        .map_err(|_| ERROR_NOT_A_DIR.to_string())?;
    if !resolved.is_dir() {
        return Err(ERROR_NOT_A_DIR.to_string());
    }

    // No parent means the filesystem root itself (`/`, `C:\`).
    if resolved.parent().is_none() {
        return Err(ERROR_PROTECTED.to_string());
    }
    if PROTECTED_EXACT
        .iter()
        .any(|entry| resolved.as_path() == Path::new(entry))
    {
        return Err(ERROR_PROTECTED.to_string());
    }
    if PROTECTED_PREFIXES
        .iter()
        .any(|prefix| resolved.starts_with(*prefix))
    {
        return Err(ERROR_PROTECTED.to_string());
    }
    // A whole mounted volume (/Volumes/<name>) is "the whole disk" for an external drive.
    if resolved.parent() == Some(Path::new("/Volumes")) {
        return Err(ERROR_PROTECTED.to_string());
    }
    if let Some(home) = home {
        let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
        if resolved == home {
            return Err(ERROR_PROTECTED.to_string());
        }
    }
    let config_dir = config_dir
        .canonicalize()
        .unwrap_or_else(|_| config_dir.to_path_buf());
    if config_dir.starts_with(&resolved) || resolved.starts_with(&config_dir) {
        return Err(ERROR_PROTECTED.to_string());
    }
    if is_network_path(&resolved) {
        return Err(ERROR_NETWORK.to_string());
    }

    Ok(resolved)
}

// Display name shown on the phone: the folder's own name. The root id disambiguates duplicates.
fn root_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

// Authorize a new root. Returns the stored entry.
pub fn add_root(
    config_dir: &Path,
    path: &Path,
    home: Option<&Path>,
) -> Result<RemoteRoot, String> {
    let resolved = guard_root(path, config_dir, home)?;
    let resolved_str = resolved.to_string_lossy().into_owned();

    let mut config = read_config_from(config_dir)?;
    if config.roots.iter().any(|root| root.path == resolved_str) {
        return Err(ERROR_DUPLICATE.to_string());
    }
    let root = RemoteRoot {
        id: uuid::Uuid::new_v4().to_string(),
        name: root_name(&resolved),
        path: resolved_str,
    };
    config.roots.push(root.clone());
    write_config_to(config_dir, &config)?;
    Ok(root)
}

pub fn remove_root(config_dir: &Path, id: &str) -> Result<(), String> {
    let mut config = read_config_from(config_dir)?;
    let before = config.roots.len();
    config.roots.retain(|root| root.id != id);
    if config.roots.len() == before {
        return Err(ERROR_UNKNOWN_ROOT.to_string());
    }
    write_config_to(config_dir, &config)
}

// Called only after the user confirmed the SAS on the desktop (remote/pairing.rs).
pub fn add_device(config_dir: &Path, device: PairedDevice) -> Result<(), String> {
    let mut config = read_config_from(config_dir)?;
    if config.device(&device.id).is_some() {
        return Err(ERROR_DEVICE_DUPLICATE.to_string());
    }
    config.devices.push(device);
    write_config_to(config_dir, &config)
}

// Local revocation: the device's key stops being trusted as soon as this returns. The caller also
// revokes it on sito-auth (DELETE /devices/{id}) and drops any live session.
pub fn remove_device(config_dir: &Path, id: &str) -> Result<(), String> {
    let mut config = read_config_from(config_dir)?;
    let before = config.devices.len();
    config.devices.retain(|device| device.id != id);
    if config.devices.len() == before {
        return Err(ERROR_UNKNOWN_DEVICE.to_string());
    }
    write_config_to(config_dir, &config)
}

pub fn set_desktop_device_id(config_dir: &Path, id: Option<String>) -> Result<(), String> {
    let mut config = read_config_from(config_dir)?;
    config.desktop_device_id = id;
    write_config_to(config_dir, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fresh scratch tree per test: <tmp>/sfb-remote-store-<name>-<pid>/{config,home,data}.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sfb-remote-store-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home/config")).unwrap();
        std::fs::create_dir_all(dir.join("data/photos")).unwrap();
        dir
    }

    #[test]
    fn rejects_relative_root() {
        let dir = scratch("relative");
        let result = guard_root(Path::new("data"), &dir.join("home/config"), None);
        assert_eq!(result, Err(ERROR_NOT_ABSOLUTE.to_string()));
    }

    #[test]
    fn rejects_filesystem_root_home_and_config_ancestors() {
        let dir = scratch("protected");
        let config = dir.join("home/config");
        let home = dir.join("home");
        for candidate in [PathBuf::from("/"), home.clone(), config.clone(), dir.clone()] {
            let result = guard_root(&candidate, &config, Some(&home));
            assert_eq!(result, Err(ERROR_PROTECTED.to_string()), "{candidate:?}");
        }
    }

    #[test]
    fn rejects_root_inside_config_dir() {
        let dir = scratch("inside-config");
        let config = dir.join("home/config");
        std::fs::create_dir_all(config.join("thumbnails")).unwrap();
        let result = guard_root(&config.join("thumbnails"), &config, None);
        assert_eq!(result, Err(ERROR_PROTECTED.to_string()));
    }

    #[test]
    fn adds_removes_and_dedupes_roots() {
        let dir = scratch("roots");
        let config = dir.join("home/config");
        let photos = dir.join("data/photos");

        let root = add_root(&config, &photos, None).unwrap();
        assert_eq!(root.name, "photos");
        // Same folder through a `..` alias still collapses to the canonical entry.
        let alias = dir.join("data/photos/../photos");
        assert_eq!(
            add_root(&config, &alias, None),
            Err(ERROR_DUPLICATE.to_string())
        );

        remove_root(&config, &root.id).unwrap();
        assert!(read_config_from(&config).unwrap().roots.is_empty());
        assert_eq!(
            remove_root(&config, &root.id),
            Err(ERROR_UNKNOWN_ROOT.to_string())
        );
    }

    #[test]
    fn corrupt_file_is_an_error_not_a_reset() {
        let dir = scratch("corrupt");
        let config = dir.join("home/config");
        std::fs::write(config.join(REMOTE_FILE), "roots = 7").unwrap();
        assert!(read_config_from(&config).is_err());
    }
}
