use std::fmt;

/// Clipboard errors. Headless / unsupported environments must surface these
/// instead of pretending success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// No `DISPLAY` / `WAYLAND_DISPLAY` (Linux) or equivalent.
    NoDisplay,
    /// Target OS not implemented yet.
    UnsupportedPlatform,
    /// Backend opened but operation failed.
    Backend(String),
    /// Content exists but is not plain text (or cleared mid-read).
    NotText,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDisplay => write!(f, "no display server for clipboard access"),
            Self::UnsupportedPlatform => {
                write!(f, "clipboard backend not implemented on this platform")
            }
            Self::Backend(msg) => write!(f, "clipboard backend error: {msg}"),
            Self::NotText => write!(f, "clipboard content is not plain text"),
        }
    }
}

impl std::error::Error for ClipboardError {}
