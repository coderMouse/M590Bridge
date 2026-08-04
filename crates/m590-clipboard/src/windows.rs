//! Windows text/image clipboard via `arboard` (Win32).

use crate::arboard_text::{
    open_clipboard, read_image_raw, read_text_raw, write_image_raw, write_text_raw,
};
use crate::{ClipboardBackend, ClipboardError, ClipboardService, ImageClipboard};

pub struct WindowsClipboard {
    clipboard: arboard::Clipboard,
    last_seen: Option<String>,
    last_image_fp: Option<u64>,
}

impl std::fmt::Debug for WindowsClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsClipboard")
            .field("backend", &ClipboardBackend::Windows)
            .field("last_seen", &self.last_seen)
            .field("last_image_fp", &self.last_image_fp)
            .finish_non_exhaustive()
    }
}

impl WindowsClipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        let mut clipboard = open_clipboard()?;
        let last_seen = read_text_raw(&mut clipboard)?;
        let last_image_fp = read_image_raw(&mut clipboard)?.as_ref().map(|img| img.fingerprint());
        Ok(Self {
            clipboard,
            last_seen,
            last_image_fp,
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
    Ok(ClipboardBackend::Windows)
}
