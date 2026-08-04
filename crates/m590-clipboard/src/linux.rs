//! Linux text/image clipboard via `arboard`.
//!
//! Detection order:
//! 1. `WAYLAND_DISPLAY` → [`ClipboardBackend::Wayland`]
//! 2. `DISPLAY` → [`ClipboardBackend::X11`]
//! 3. else [`ClipboardError::NoDisplay`]

use crate::arboard_text::{
    open_clipboard, read_image_raw, read_text_raw, write_image_raw, write_text_raw,
};
use crate::{ClipboardBackend, ClipboardError, ClipboardService, ImageClipboard};

pub struct LinuxClipboard {
    backend: ClipboardBackend,
    clipboard: arboard::Clipboard,
    last_seen: Option<String>,
    last_image_fp: Option<u64>,
}

impl std::fmt::Debug for LinuxClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxClipboard")
            .field("backend", &self.backend)
            .field("last_seen", &self.last_seen)
            .field("last_image_fp", &self.last_image_fp)
            .finish_non_exhaustive()
    }
}

impl LinuxClipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        let backend = detect_backend()?;
        let mut clipboard = open_clipboard()?;
        let last_seen = read_text_raw(&mut clipboard)?;
        let last_image_fp = read_image_raw(&mut clipboard)?.as_ref().map(|img| img.fingerprint());
        Ok(Self {
            backend,
            clipboard,
            last_seen,
            last_image_fp,
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

    fn read_image(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        read_image_raw(&mut self.clipboard)
    }

    fn write_image(&mut self, image: &ImageClipboard) -> Result<(), ClipboardError> {
        write_image_raw(&mut self.clipboard, image)?;
        self.last_image_fp = Some(image.fingerprint());
        Ok(())
    }

    fn poll_image_change(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        let current = read_image_raw(&mut self.clipboard)?;
        let fp = current.as_ref().map(|img| img.fingerprint());
        if fp != self.last_image_fp {
            self.last_image_fp = fp;
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
