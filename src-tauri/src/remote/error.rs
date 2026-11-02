use serde::Serialize;

// Typed errors sent back to the phone inside the encrypted channel. A closed set on purpose: the
// phone never receives OS error prose (it could leak absolute paths or usernames), only a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemoteError {
    // Outside an authorized root, an unknown root, a malformed path, or an OS permission denial.
    Forbidden,
    NotFound,
    // Over the per-file download limit.
    TooLarge,
    RateLimited,
    Internal,
}

impl RemoteError {
    // Map an io error from the filesystem to the closest code. Anything unexpected is INTERNAL so
    // the phone cannot probe the disk by comparing error variants.
    pub fn from_io(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => RemoteError::NotFound,
            std::io::ErrorKind::PermissionDenied => RemoteError::Forbidden,
            _ => RemoteError::Internal,
        }
    }
}
