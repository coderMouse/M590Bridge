//! Linux text clipboard via `arboard`.
//!
//! Detection order:
//! 1. `WAYLAND_DISPLAY` → [`ClipboardBackend::Wayland`]
//! 2. `DISPLAY` → [`ClipboardBackend::X11`]
//! 3. else [`ClipboardError::NoDisplay`]

use crate::arboard_text::{open_clipboard, read_text_raw, write_text_raw};
use crate::{ClipboardBackend, ClipboardError, ClipboardService};

pub struct LinuxClipboard {
    backend: ClipboardBackend,
    clipboard: arboard::Clipboard,
    last_seen: Option<String>,
}

impl std::fmt::Debug for LinuxClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxClipboard")
            .field("backend", &self.backend)
            .field("last_seen", &self.last_seen)
            .finish_non_exhaustive()
    }
}

impl LinuxClipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        let backend = detect_backend()?;
        let mut clipboard = open_clipboard()?;
        let last_seen = read_text_raw(&mut clipboard)?;
        Ok(Self {
            backend,
            clipboard,
            last_seen,
        })
    }

    pub fn backend(&self) -> ClipboardBackend {
        self.backend
    }
}

impl ClipboardService for LinuxClipboard {
    fn backend(&self) -> ClipboardBackend {
        self.backend
    }

    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        read_text_raw(&mut self.clipboard)
    }

    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        write_text_raw(&mut self.clipboard, text)?;
        self.last_seen = Some(text.to_string());
        Ok(())
    }

    fn poll_text_change(&mut self) -> Result<Option<String>, ClipboardError> {
        let current = read_text_raw(&mut self.clipboard)?;
        if current != self.last_seen {
            self.last_seen = current.clone();
            Ok(current)
        } else {
            Ok(None)
        }
    }
}

pub fn detect_backend() -> Result<ClipboardBackend, ClipboardError> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").filter(|v| !v.is_empty());
    if wayland.is_some() {
        return Ok(ClipboardBackend::Wayland);
    }
    let x11 = std::env::var_os("DISPLAY").filter(|v| !v.is_empty());
    if x11.is_some() {
        return Ok(ClipboardBackend::X11);
    }
    Err(ClipboardError::NoDisplay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_backend_matches_env_policy() {
        let has_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        let has_x11 = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
        match detect_backend() {
            Ok(ClipboardBackend::Wayland) => assert!(has_wayland),
            Ok(ClipboardBackend::X11) => {
                assert!(has_x11);
                assert!(!has_wayland);
            }
            Ok(other) => panic!("unexpected backend {other:?}"),
            Err(ClipboardError::NoDisplay) => {
                assert!(!has_wayland && !has_x11);
            }
            Err(err) => panic!("unexpected error {err}"),
        }
    }
}
