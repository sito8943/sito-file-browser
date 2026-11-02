use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::filesystem::fs::{
    get_entry_local, read_directory_local, remote_thumbnail, search_directory_core, DirEntry,
};

use super::error::RemoteError;
use super::paths::{has_hidden_component, is_wire_safe, resolve_in_root, to_relative};
use super::store::{RemoteConfig, RemoteRoot};

// Largest chunk one `file.read` returns (plan: 256 KB). Base64 inside the JSON request, then again
// around the ciphertext in the relay envelope, keeps one chunk under the relay's 512 KB frame cap.
pub const FILE_CHUNK_MAX: u64 = 256 * 1024;
// Per-file download limit (plan decision 3).
pub const FILE_SIZE_MAX: u64 = 500 * 1024 * 1024;
// Search hits returned to the phone. Lower than the UI's cap so one response fits a relay frame.
const SEARCH_RESULT_MAX: usize = 200;
// Thumbnail edge clamp: below is useless, above risks exceeding a relay frame as a JPEG.
const THUMB_SIZE_MIN: u32 = 64;
const THUMB_SIZE_MAX: u32 = 512;
const THUMB_MIME: &str = "image/jpeg";

// Per-session serving options read from the user's settings when a session starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServeOptions {
    // Whether the phone may see and open dotfiles (setting `remoteShowHidden`, default false).
    // Off: hidden entries are left out of listings/search and requesting one answers NOT_FOUND,
    // so the phone cannot even confirm that a `.env` or `.git` exists.
    pub show_hidden: bool,
}

// The closed allowlist of requests (security rule 1: read-only + copy-to-phone). Anything that does
// not deserialize into one of these variants — an unknown `op`, a write verb, a missing field — is
// answered FORBIDDEN by the session layer without touching the disk.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum Request {
    #[serde(rename = "roots.list")]
    RootsList,
    #[serde(rename = "dir.list")]
    DirList { root: String, path: String },
    #[serde(rename = "entry.stat")]
    EntryStat { root: String, path: String },
    #[serde(rename = "search")]
    Search {
        root: String,
        path: String,
        query: String,
    },
    #[serde(rename = "thumb.get")]
    ThumbGet { root: String, path: String, size: u32 },
    #[serde(rename = "file.read")]
    FileRead {
        root: String,
        path: String,
        offset: u64,
        len: u64,
    },
}

// What the phone sees of a root: never its absolute path.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RootInfo {
    pub id: String,
    pub name: String,
}

// What the phone sees of an entry. `path` is relative to the request's root, in wire form.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    // Unix milliseconds. The phone resends it to detect a file that changed mid-download.
    pub modified: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Response {
    Roots {
        roots: Vec<RootInfo>,
    },
    Entries {
        entries: Vec<RemoteEntry>,
    },
    Entry {
        entry: RemoteEntry,
    },
    Thumb {
        mime: String,
        data: String,
    },
    #[serde(rename_all = "camelCase")]
    Chunk {
        offset: u64,
        // Base64 of the bytes read; empty at end of file.
        data: String,
        total_size: u64,
        modified: u64,
        eof: bool,
    },
}

// Entry point for one decrypted request from a paired device. Every filesystem touch runs on the
// blocking pool, never on Tauri's main thread or the relay socket task.
pub async fn handle(
    app: &AppHandle,
    config: &RemoteConfig,
    options: ServeOptions,
    request: Request,
) -> Result<Response, RemoteError> {
    match request {
        Request::ThumbGet { root, path, size } => {
            let root = find_root(config, &root)?.clone();
            check_visible(options, &path)?;
            thumbnail(app, root, path, size).await
        }
        other => {
            let config = config.clone();
            tauri::async_runtime::spawn_blocking(move || {
                handle_blocking(&config, options, other)
            })
            .await
            .map_err(|_| RemoteError::Internal)?
        }
    }
}

// Every request except thumbnails, synchronously. Split out so it can be unit-tested without a
// Tauri app; callers must already be on a blocking thread.
pub fn handle_blocking(
    config: &RemoteConfig,
    options: ServeOptions,
    request: Request,
) -> Result<Response, RemoteError> {
    match &request {
        Request::DirList { path, .. }
        | Request::EntryStat { path, .. }
        | Request::Search { path, .. }
        | Request::ThumbGet { path, .. }
        | Request::FileRead { path, .. } => check_visible(options, path)?,
        Request::RootsList => {}
    }
    match request {
        Request::RootsList => Ok(Response::Roots {
            roots: config
                .roots
                .iter()
                .map(|root| RootInfo {
                    id: root.id.clone(),
                    name: root.name.clone(),
                })
                .collect(),
        }),
        Request::DirList { root, path } => {
            let root = find_root(config, &root)?;
            let dir = resolve_in_root(root, &path)?;
            if !dir.is_dir() {
                return Err(RemoteError::NotFound);
            }
            let entries = read_directory_local(&dir.to_string_lossy()).map_err(core_error)?;
            Ok(Response::Entries {
                entries: to_remote_entries(root, options, entries),
            })
        }
        Request::EntryStat { root, path } => {
            let root = find_root(config, &root)?;
            let target = resolve_in_root(root, &path)?;
            let entry =
                get_entry_local(target.to_string_lossy().into_owned()).map_err(core_error)?;
            // The resolved path is canonical, so map it back through the root; the phone keeps
            // the relative path it asked for when that differs (a symlink inside the root).
            let mut remote = to_remote_entry(Path::new(&root.path), &entry, &target)
                .ok_or(RemoteError::Forbidden)?;
            remote.path = path;
            Ok(Response::Entry { entry: remote })
        }
        Request::Search { root, path, query } => {
            let root = find_root(config, &root)?;
            let dir = resolve_in_root(root, &path)?;
            if !dir.is_dir() {
                return Err(RemoteError::NotFound);
            }
            let hits =
                search_directory_core(&dir.to_string_lossy(), &query).map_err(core_error)?;
            // Truncate after filtering so hidden hits don't eat the result budget.
            let mut entries = to_remote_entries(root, options, hits);
            entries.truncate(SEARCH_RESULT_MAX);
            Ok(Response::Entries { entries })
        }
        Request::FileRead {
            root,
            path,
            offset,
            len,
        } => read_chunk(find_root(config, &root)?, &path, offset, len),
        // Routed to the async thumbnail path by `handle`; reaching here is a caller bug.
        Request::ThumbGet { .. } => Err(RemoteError::Internal),
    }
}

fn check_visible(options: ServeOptions, relative: &str) -> Result<(), RemoteError> {
    if !options.show_hidden && has_hidden_component(relative) {
        return Err(RemoteError::NotFound);
    }
    Ok(())
}

fn find_root<'a>(config: &'a RemoteConfig, id: &str) -> Result<&'a RemoteRoot, RemoteError> {
    config.root(id).ok_or(RemoteError::Forbidden)
}

// Core errors are prose strings; only the permission marker is meaningful to the phone.
fn core_error(error: String) -> RemoteError {
    if error == "ACCESS_DENIED" {
        RemoteError::Forbidden
    } else {
        RemoteError::Internal
    }
}

fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

// Convert listing/search results, dropping anything the phone must not see: entries whose
// symlinks resolve outside the root (security rule 2 — they would be listed but unreadable, and
// their metadata describes a file outside the root), names the wire grammar cannot express, and
// hidden entries unless the user allowed them.
fn to_remote_entries(
    root: &RemoteRoot,
    options: ServeOptions,
    entries: Vec<DirEntry>,
) -> Vec<RemoteEntry> {
    let root_path = Path::new(&root.path);
    entries
        .iter()
        .filter_map(|entry| {
            let canonical = entry.path.canonicalize().ok()?;
            if !canonical.starts_with(root_path) {
                return None;
            }
            let remote = to_remote_entry(root_path, entry, &entry.path)?;
            if !options.show_hidden && has_hidden_component(&remote.path) {
                return None;
            }
            Some(remote)
        })
        .collect()
}

// `listed` is the path under the root as listed (lexical, not canonical), so a symlink inside the
// root keeps its own name and location instead of its target's.
fn to_remote_entry(root_path: &Path, entry: &DirEntry, listed: &Path) -> Option<RemoteEntry> {
    let relative = to_relative(root_path, listed)?;
    if !is_wire_safe(&relative) {
        return None;
    }
    Some(RemoteEntry {
        name: entry.name.clone(),
        path: relative,
        size: entry.size,
        is_dir: entry.metadata.is_dir,
        is_file: entry.metadata.is_file,
        modified: unix_millis(entry.metadata.modified),
    })
}

// One resumable download chunk. Reads through the canonical path resolved just before opening.
fn read_chunk(
    root: &RemoteRoot,
    relative: &str,
    offset: u64,
    len: u64,
) -> Result<Response, RemoteError> {
    let target = resolve_in_root(root, relative)?;
    // Checked before opening: Windows refuses to open a directory as a file with a permission
    // error, which would surface as FORBIDDEN instead of NOT_FOUND.
    if !target.is_file() {
        return Err(RemoteError::NotFound);
    }
    let mut file = std::fs::File::open(&target).map_err(|e| RemoteError::from_io(&e))?;
    let metadata = file.metadata().map_err(|e| RemoteError::from_io(&e))?;
    if !metadata.is_file() {
        return Err(RemoteError::NotFound);
    }
    let total_size = metadata.len();
    if total_size > FILE_SIZE_MAX {
        return Err(RemoteError::TooLarge);
    }
    let modified = unix_millis(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));

    let start = offset.min(total_size);
    let want = len.min(FILE_CHUNK_MAX).min(total_size - start);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| RemoteError::from_io(&e))?;
    let mut buffer = Vec::with_capacity(want as usize);
    file.take(want)
        .read_to_end(&mut buffer)
        .map_err(|e| RemoteError::from_io(&e))?;

    Ok(Response::Chunk {
        offset: start,
        eof: start + buffer.len() as u64 >= total_size,
        data: BASE64.encode(&buffer),
        total_size,
        modified,
    })
}

async fn thumbnail(
    app: &AppHandle,
    root: RemoteRoot,
    relative: String,
    size: u32,
) -> Result<Response, RemoteError> {
    let target = tauri::async_runtime::spawn_blocking(move || resolve_in_root(&root, &relative))
        .await
        .map_err(|_| RemoteError::Internal)??;
    if !target.is_file() {
        return Err(RemoteError::NotFound);
    }
    let size = size.clamp(THUMB_SIZE_MIN, THUMB_SIZE_MAX);
    // The generator reports unsupported formats as prose; to the phone that is just "no thumbnail".
    let cached = remote_thumbnail(app.clone(), target.to_string_lossy().into_owned(), size)
        .await
        .map_err(|_| RemoteError::NotFound)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(cached))
        .await
        .map_err(|_| RemoteError::Internal)?
        .map_err(|e| RemoteError::from_io(&e))?;
    if bytes.len() as u64 > FILE_CHUNK_MAX {
        return Err(RemoteError::TooLarge);
    }
    Ok(Response::Thumb {
        mime: THUMB_MIME.to_string(),
        data: BASE64.encode(&bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // <tmp>/sfb-remote-handlers-<name>-<pid>/{root/{a.txt,sub/b.txt}, outside/secret.txt}
    fn scratch(name: &str) -> (PathBuf, RemoteConfig) {
        let dir = std::env::temp_dir().join(format!(
            "sfb-remote-handlers-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("root/sub")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        std::fs::write(dir.join("root/a.txt"), "hello world").unwrap();
        std::fs::write(dir.join("root/sub/b.txt"), "b").unwrap();
        std::fs::write(dir.join("outside/secret.txt"), "no").unwrap();
        let config = RemoteConfig {
            desktop_device_id: None,
            roots: vec![RemoteRoot {
                id: "r1".to_string(),
                name: "root".to_string(),
                path: dir
                    .join("root")
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            }],
            devices: Vec::new(),
        };
        (dir, config)
    }

    fn paths(response: Response) -> Vec<String> {
        match response {
            Response::Entries { entries } => {
                let mut paths: Vec<String> = entries.into_iter().map(|e| e.path).collect();
                paths.sort();
                paths
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn allowlist_rejects_unknown_and_write_ops() {
        for raw in [
            r#"{"op":"file.delete","root":"r1","path":"a.txt"}"#,
            r#"{"op":"file.write","root":"r1","path":"a.txt"}"#,
            r#"{"op":"dir.list","root":"r1"}"#,
            r#"{"path":"a.txt"}"#,
        ] {
            assert!(serde_json::from_str::<Request>(raw).is_err(), "{raw}");
        }
        let ok: Request =
            serde_json::from_str(r#"{"op":"dir.list","root":"r1","path":""}"#).unwrap();
        assert_eq!(
            ok,
            Request::DirList {
                root: "r1".to_string(),
                path: String::new()
            }
        );
    }

    #[test]
    fn roots_never_expose_absolute_paths() {
        let (_dir, config) = scratch("roots");
        let roots =
            handle_blocking(&config, ServeOptions::default(), Request::RootsList).unwrap();
        let json = serde_json::to_string(&roots).unwrap();
        assert!(!json.contains(&config.roots[0].path), "{json}");
    }

    #[test]
    fn lists_relative_entries_and_unknown_root_is_forbidden() {
        let (_dir, config) = scratch("list");
        let listing = handle_blocking(
            &config,
            ServeOptions::default(),
            Request::DirList {
                root: "r1".to_string(),
                path: String::new(),
            },
        )
        .unwrap();
        assert_eq!(paths(listing), vec!["a.txt", "sub"]);

        let nested = handle_blocking(
            &config,
            ServeOptions::default(),
            Request::DirList {
                root: "r1".to_string(),
                path: "sub".to_string(),
            },
        )
        .unwrap();
        assert_eq!(paths(nested), vec!["sub/b.txt"]);

        let unknown = handle_blocking(
            &config,
            ServeOptions::default(),
            Request::DirList {
                root: "nope".to_string(),
                path: String::new(),
            },
        );
        assert_eq!(unknown, Err(RemoteError::Forbidden));
    }

    #[cfg(unix)]
    #[test]
    fn listing_hides_symlinks_that_escape_root() {
        let (dir, config) = scratch("escape");
        std::os::unix::fs::symlink(dir.join("outside"), dir.join("root/link")).unwrap();
        let listing = handle_blocking(
            &config,
            ServeOptions::default(),
            Request::DirList {
                root: "r1".to_string(),
                path: String::new(),
            },
        )
        .unwrap();
        assert_eq!(paths(listing), vec!["a.txt", "sub"]);
    }

    #[test]
    fn search_returns_relative_hits() {
        let (_dir, config) = scratch("search");
        let hits = handle_blocking(
            &config,
            ServeOptions::default(),
            Request::Search {
                root: "r1".to_string(),
                path: String::new(),
                query: "B.TXT".to_string(),
            },
        )
        .unwrap();
        assert_eq!(paths(hits), vec!["sub/b.txt"]);
    }

    #[test]
    fn reads_file_in_chunks() {
        let (_dir, config) = scratch("chunks");
        let read = |offset, len| {
            handle_blocking(
                &config,
                ServeOptions::default(),
                Request::FileRead {
                    root: "r1".to_string(),
                    path: "a.txt".to_string(),
                    offset,
                    len,
                },
            )
            .unwrap()
        };
        match read(6, 100) {
            Response::Chunk {
                offset,
                data,
                total_size,
                eof,
                ..
            } => {
                assert_eq!(offset, 6);
                assert_eq!(BASE64.decode(data).unwrap(), b"world");
                assert_eq!(total_size, 11);
                assert!(eof);
            }
            other => panic!("unexpected {other:?}"),
        }
        match read(0, 5) {
            Response::Chunk { data, eof, .. } => {
                assert_eq!(BASE64.decode(data).unwrap(), b"hello");
                assert!(!eof);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn hidden_entries_follow_the_setting() {
        let (dir, config) = scratch("hidden");
        std::fs::create_dir_all(dir.join("root/.git")).unwrap();
        std::fs::write(dir.join("root/.env"), "SECRET=1").unwrap();
        let list = |show_hidden| {
            handle_blocking(
                &config,
                ServeOptions { show_hidden },
                Request::DirList {
                    root: "r1".to_string(),
                    path: String::new(),
                },
            )
            .unwrap()
        };
        assert_eq!(paths(list(false)), vec!["a.txt", "sub"]);
        assert_eq!(paths(list(true)), vec![".env", ".git", "a.txt", "sub"]);

        let read_env = |show_hidden| {
            handle_blocking(
                &config,
                ServeOptions { show_hidden },
                Request::FileRead {
                    root: "r1".to_string(),
                    path: ".env".to_string(),
                    offset: 0,
                    len: 10,
                },
            )
        };
        assert_eq!(read_env(false), Err(RemoteError::NotFound));
        assert!(read_env(true).is_ok());
    }

    #[test]
    fn reading_a_directory_is_not_found() {
        let (_dir, config) = scratch("read-dir");
        let result = handle_blocking(
            &config,
            ServeOptions::default(),
            Request::FileRead {
                root: "r1".to_string(),
                path: "sub".to_string(),
                offset: 0,
                len: 10,
            },
        );
        assert_eq!(result, Err(RemoteError::NotFound));
    }
}
