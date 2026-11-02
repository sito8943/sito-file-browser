use std::path::{Component, Path, PathBuf};

use super::error::RemoteError;
use super::store::RemoteRoot;
use super::volume::is_network_path;

// Separator of relative paths on the wire. Fixed to `/` on every desktop OS so the phone never has
// to know the desktop's platform.
const WIRE_SEPARATOR: char = '/';

// Characters never valid inside a wire component. `\` would let a Windows desktop read a component
// as a separator (refused everywhere so wire paths mean the same on every OS); NUL is rejected by
// the OS anyway but refusing early keeps the error FORBIDDEN instead of INTERNAL. On Windows `:` is
// refused too: it would turn a component into a drive (`C:`) or an NTFS alternate data stream
// (`file:stream`). macOS filenames may legitimately contain `:`, so it stays valid there.
#[cfg(windows)]
const FORBIDDEN_CHARS: &[char] = &['\\', ':', '\0'];
#[cfg(not(windows))]
const FORBIDDEN_CHARS: &[char] = &['\\', '\0'];

// Resolve a request's `{root, path}` into an absolute, canonical path that is guaranteed to sit
// inside the authorized root (security rule 2). `relative` uses `/` separators; `""` is the root.
//
// Defense in depth, in order:
// 1. syntactic: no absolute paths, no `.`/`..`/empty components, no separator/drive characters;
// 2. the root itself is re-canonicalized and must still equal the stored canonical path — a root
//    that was replaced by a symlink after being authorized is refused instead of followed;
// 3. the joined path is canonicalized (resolving every symlink) and must still start with the
//    root, so a symlink inside the root that points outside it is FORBIDDEN;
// 4. the target must not sit on a network filesystem: a share mounted on a folder inside the root
//    after it was authorized is still refused.
//
// Remaining gap, accepted for 0.18.0: a symlink swapped in between this check and the open (TOCTOU)
// can still redirect one read. Callers open the returned canonical path, which had no symlinks at
// check time, which narrows but does not close that window.
pub fn resolve_in_root(root: &RemoteRoot, relative: &str) -> Result<PathBuf, RemoteError> {
    let components = parse_relative(relative)?;

    let stored = Path::new(&root.path);
    let canonical_root = stored.canonicalize().map_err(|e| RemoteError::from_io(&e))?;
    if canonical_root != stored {
        return Err(RemoteError::Forbidden);
    }

    let mut joined = canonical_root.clone();
    for component in components {
        joined.push(component);
    }
    let canonical = joined.canonicalize().map_err(|e| RemoteError::from_io(&e))?;
    if !canonical.starts_with(&canonical_root) || is_network_path(&canonical) {
        return Err(RemoteError::Forbidden);
    }
    Ok(canonical)
}

// Inverse of resolve_in_root for listings: express an absolute path found under the root (a
// directory child, a search hit) as the wire-relative path the phone may send back. None when the
// path is not inside the root, so callers drop that entry instead of leaking an absolute path.
pub fn to_relative(canonical_root: &Path, absolute: &Path) -> Option<String> {
    let rest = absolute.strip_prefix(canonical_root).ok()?;
    let mut parts = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_string()),
            _ => return None,
        }
    }
    Some(parts.join(&WIRE_SEPARATOR.to_string()))
}

// Whether any component of a wire path is hidden (dotfile convention, same rule the directory view
// uses). The root itself (`""`) is never hidden: the user authorized it explicitly.
pub fn has_hidden_component(relative: &str) -> bool {
    relative
        .split(WIRE_SEPARATOR)
        .any(|component| component.starts_with('.'))
}

// Whether a relative path built from a listing can be sent back by the phone. Entries whose names
// the wire grammar cannot express (e.g. a `\` in a Unix filename) are dropped from listings instead
// of being shown as unreachable.
pub fn is_wire_safe(relative: &str) -> bool {
    parse_relative(relative).is_ok()
}

fn parse_relative(relative: &str) -> Result<Vec<&str>, RemoteError> {
    if relative.is_empty() {
        return Ok(Vec::new());
    }
    if relative.starts_with(WIRE_SEPARATOR) {
        return Err(RemoteError::Forbidden);
    }
    let mut components = Vec::new();
    for component in relative.split(WIRE_SEPARATOR) {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.contains(FORBIDDEN_CHARS)
        {
            return Err(RemoteError::Forbidden);
        }
        components.push(component);
    }
    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;

    // <tmp>/sfb-remote-paths-<name>-<pid>/{root/sub/file.txt, outside/secret.txt}
    fn scratch(name: &str) -> (PathBuf, RemoteRoot) {
        let dir = std::env::temp_dir().join(format!(
            "sfb-remote-paths-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("root/sub")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        std::fs::write(dir.join("root/sub/file.txt"), "ok").unwrap();
        std::fs::write(dir.join("outside/secret.txt"), "no").unwrap();
        let canonical = dir.join("root").canonicalize().unwrap();
        let root = RemoteRoot {
            id: "r1".to_string(),
            name: "root".to_string(),
            path: canonical.to_string_lossy().into_owned(),
        };
        (dir, root)
    }

    #[test]
    fn resolves_root_and_children() {
        let (_dir, root) = scratch("children");
        let base = PathBuf::from(&root.path);
        assert_eq!(resolve_in_root(&root, ""), Ok(base.clone()));
        assert_eq!(
            resolve_in_root(&root, "sub/file.txt"),
            Ok(base.join("sub/file.txt"))
        );
        assert_eq!(
            resolve_in_root(&root, "sub/missing"),
            Err(RemoteError::NotFound)
        );
    }

    #[test]
    fn rejects_malformed_paths() {
        let (_dir, root) = scratch("malformed");
        for bad in [
            "/etc/passwd",
            "..",
            "../outside/secret.txt",
            "sub/../../outside",
            "sub/./file.txt",
            "sub//file.txt",
            "sub/",
            "sub\\..\\..",
            "sub/\0",
        ] {
            assert_eq!(
                resolve_in_root(&root, bad),
                Err(RemoteError::Forbidden),
                "{bad:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_drive_and_stream_components_on_windows() {
        let (_dir, root) = scratch("windows");
        for bad in ["C:", "sub/file.txt:stream"] {
            assert_eq!(
                resolve_in_root(&root, bad),
                Err(RemoteError::Forbidden),
                "{bad:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_root() {
        let (dir, root) = scratch("symlink");
        std::os::unix::fs::symlink(dir.join("outside"), dir.join("root/link")).unwrap();
        assert_eq!(
            resolve_in_root(&root, "link/secret.txt"),
            Err(RemoteError::Forbidden)
        );
    }

    #[cfg(unix)]
    #[test]
    fn allows_symlink_staying_inside_root() {
        let (dir, root) = scratch("symlink-inside");
        std::os::unix::fs::symlink(dir.join("root/sub"), dir.join("root/alias")).unwrap();
        let base = PathBuf::from(&root.path);
        assert_eq!(
            resolve_in_root(&root, "alias/file.txt"),
            Ok(base.join("sub/file.txt"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_root_swapped_for_symlink() {
        let (dir, root) = scratch("swapped");
        std::fs::remove_dir_all(dir.join("root")).unwrap();
        std::os::unix::fs::symlink(dir.join("outside"), dir.join("root")).unwrap();
        assert_eq!(resolve_in_root(&root, ""), Err(RemoteError::Forbidden));
    }

    #[test]
    fn detects_hidden_components() {
        assert!(!has_hidden_component(""));
        assert!(!has_hidden_component("sub/file.txt"));
        assert!(has_hidden_component(".git"));
        assert!(has_hidden_component("app/.env"));
        assert!(has_hidden_component("a/.cache/b"));
    }

    #[test]
    fn relative_round_trip() {
        let (_dir, root) = scratch("relative");
        let base = PathBuf::from(&root.path);
        assert_eq!(to_relative(&base, &base), Some(String::new()));
        assert_eq!(
            to_relative(&base, &base.join("sub/file.txt")),
            Some("sub/file.txt".to_string())
        );
        assert_eq!(to_relative(&base, Path::new("/elsewhere")), None);
    }
}
