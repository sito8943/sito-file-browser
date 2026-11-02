use std::path::Path;

// Whether `path` lives on a network filesystem (SMB, NFS, AFP, WebDAV, mapped drive...). Remote
// control never serves those (plan: "no SFTP/SMB through remote"): exposing a share the desktop
// mounted would hand the phone access to a third machine, and a dead mount would hang a worker.
//
// Fails closed: if the filesystem cannot be inspected, the path is treated as network.
pub fn is_network_path(path: &Path) -> bool {
    imp::is_network_path(path)
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // MNT_LOCAL is set by the kernel for every filesystem backed by local storage; smbfs, nfs,
    // afpfs and webdav lack it.
    pub fn is_network_path(path: &Path) -> bool {
        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
            return true;
        };
        let mut info: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: c_path is a valid NUL-terminated string and info a writable statfs buffer.
        if unsafe { libc::statfs(c_path.as_ptr(), &mut info) } != 0 {
            return true;
        }
        info.f_flags & (libc::MNT_LOCAL as u32) == 0
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    // statfs f_type magics of network filesystems (linux/magic.h). Compared as u32 because the
    // field's integer type differs between architectures. FUSE is deliberately absent: it also
    // backs local filesystems (ntfs-3g, exFAT), so sshfs mounts are not detected here.
    const NETWORK_MAGICS: &[u32] = &[
        0x0000_6969, // NFS
        0x0000_517B, // SMB
        0xFF53_4D42, // CIFS
        0xFE53_4D42, // SMB2
        0x5346_414F, // AFS
        0x7375_7245, // CODA
        0x00C3_6400, // CEPH
    ];

    pub fn is_network_path(path: &Path) -> bool {
        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
            return true;
        };
        let mut info: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: c_path is a valid NUL-terminated string and info a writable statfs buffer.
        if unsafe { libc::statfs(c_path.as_ptr(), &mut info) } != 0 {
            return true;
        }
        NETWORK_MAGICS.contains(&(info.f_type as u32))
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::path::{Component, Path, Prefix};

    // Win32 DRIVE_REMOTE (GetDriveTypeW return value for network drives).
    const DRIVE_REMOTE: u32 = 4;

    // UNC paths (\\server\share, \\?\UNC\server\share) are network by definition; drive letters
    // are asked to the OS so a mapped network drive (Z:) is caught too.
    pub fn is_network_path(path: &Path) -> bool {
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return true;
        };
        let letter = match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            _ => return true,
        };
        let root: Vec<u16> = format!("{}:\\", letter as char)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: root is a NUL-terminated UTF-16 string that outlives the call.
        let kind = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) };
        kind == DRIVE_REMOTE
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod imp {
    use std::path::Path;

    // Unknown platform: fail closed.
    pub fn is_network_path(_path: &Path) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dir_is_local() {
        let dir = std::env::temp_dir().canonicalize().unwrap();
        assert!(!is_network_path(&dir));
    }

    #[cfg(unix)]
    #[test]
    fn missing_path_fails_closed() {
        assert!(is_network_path(Path::new("/definitely/not/here/sfb")));
    }
}
