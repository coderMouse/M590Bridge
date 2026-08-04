//! Windows text clipboard via `arboard` (Win32).
//!
//! Runtime verification requires a real Windows 10+ session.
//! Cross-compiling from Linux only proves the `cfg` path type-checks when a
//! Windows target toolchain is available.

use crate::arboard_text::{open_clipboard, read_text_raw, write_text_raw};
use crate::{ClipboardBackend, ClipboardError, ClipboardService};

pub struct WindowsClipboard {
    clipboard: arboard::Clipboard,
    last_seen: Option<String>,
}

impl std::fmt::Debug for WindowsClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsClipboard")
            .field("backend", &ClipboardBackend::Windows)
            .field("last_seen", &self.last_seen)
            .finish_non_exhaustive()
    }
}

impl WindowsClipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        let mut clipboard = open_clipboard()?;
        let last_seen = read_text_raw(&mut clipboard)?;
        Ok(Self {
            clipboard,
            last_seen,
        })
    }

    pub fn backend(&self) -> ClipboardBackend {
        ClipboardBackend::Windows
    }
}

impl ClipboardService for WindowsClipboard {
    fn backend(&self) -> ClipboardBackend {
        ClipboardBackend::Windows
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
    Ok(ClipboardBackend::Windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_backend_is_windows() {
        assert_eq!(detect_backend().unwrap(), ClipboardBackend::Windows);
    }

    #[test]
    fn windows_text_write_read_poll_if_clipboard_available() {
        let mut clip = match WindowsClipboard::open() {
            Ok(c) => c,
            Err(err) => {
                eprintln!("skip windows clipboard integration: {err}");
                return;
            }
        };

        let marker = format!(
            "m590-clipboard-win-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        clip.write_text(&marker)
            .expect("write_text should work after open succeeded");
        let read = clip
            .read_text()
            .expect("read_text should work after open succeeded");
        assert_eq!(read.as_deref(), Some(marker.as_str()));
        assert_eq!(clip.poll_text_change().unwrap(), None);

        if let Ok(mut other) = WindowsClipboard::open() {
            let external = format!("{marker}-external");
            other.write_text(&external).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let changed = clip.poll_text_change().unwrap();
            assert_eq!(changed.as_deref(), Some(external.as_str()));
        }
    }
}
