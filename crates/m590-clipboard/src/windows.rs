//! Windows text/image clipboard via `arboard` (Win32).

use crate::arboard_text::{
    open_clipboard, read_file_list_raw, read_image_raw, read_text_raw, write_image_raw,
    write_text_raw,
};
use crate::{ClipboardBackend, ClipboardError, ClipboardService, ImageClipboard};

pub struct WindowsClipboard {
    clipboard: arboard::Clipboard,
    last_seen: Option<String>,
    last_image_fp: Option<u64>,
    last_files: Vec<std::path::PathBuf>,
}

impl std::fmt::Debug for WindowsClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsClipboard")
            .field("backend", &ClipboardBackend::Windows)
            .field("last_seen", &self.last_seen)
            .field("last_image_fp", &self.last_image_fp)
            .field("last_files", &self.last_files)
            .finish_non_exhaustive()
    }
}

impl WindowsClipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        let mut clipboard = open_clipboard()?;
        let last_seen = read_text_raw(&mut clipboard)?;
        let last_image_fp = read_image_raw(&mut clipboard)?.as_ref().map(|img| img.fingerprint());
        let last_files = read_file_list_raw(&mut clipboard).unwrap_or_default();
        Ok(Self {
            clipboard,
            last_seen,
            last_image_fp,
            last_files,
        })
    }

    pub fn backend(&self) -> ClipboardBackend {
        ClipboardBackend::Windows
    }

    fn refresh_clipboard(&mut self) -> Result<(), ClipboardError> {
        self.clipboard = open_clipboard()?;
        Ok(())
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
        match read_image_raw(&mut self.clipboard) {
            Ok(v) => Ok(v),
            Err(err) => {
                // Windows clipboard can be transiently locked; reopen once.
                let _ = self.refresh_clipboard();
                match read_image_raw(&mut self.clipboard) {
                    Ok(v) => Ok(v),
                    Err(_) => Err(err),
                }
            }
        }
    }

    fn write_image(&mut self, image: &ImageClipboard) -> Result<(), ClipboardError> {
        write_image_raw(&mut self.clipboard, image)?;
        self.last_image_fp = Some(image.fingerprint());
        Ok(())
    }

    fn poll_image_change(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        let current = match read_image_raw(&mut self.clipboard) {
            Ok(v) => v,
            Err(_) => {
                self.refresh_clipboard()?;
                read_image_raw(&mut self.clipboard)?
            }
        };
        let fp = current.as_ref().map(|img| img.fingerprint());
        if fp != self.last_image_fp {
            self.last_image_fp = fp;
            Ok(current)
        } else {
            Ok(None)
        }
    }

    fn read_file_list(&mut self) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        read_file_list_raw(&mut self.clipboard)
    }

    fn poll_file_list_change(
        &mut self,
    ) -> Result<Option<Vec<std::path::PathBuf>>, ClipboardError> {
        let current = match read_file_list_raw(&mut self.clipboard) {
            Ok(v) => v,
            Err(_) => {
                self.refresh_clipboard()?;
                read_file_list_raw(&mut self.clipboard)?
            }
        };
        if current != self.last_files {
            self.last_files = current.clone();
            Ok(Some(current))
        } else {
            Ok(None)
        }
    }

    fn adopt_text_baseline(&mut self) {
        let current = read_text_raw(&mut self.clipboard).or_else(|_| {
            self.refresh_clipboard()?;
            read_text_raw(&mut self.clipboard)
        });
        if let Ok(v) = current {
            self.last_seen = v;
        }
    }

    fn prime_poll_to_emit_current(&mut self) {
        self.last_seen = None;
        self.last_image_fp = None;
        self.last_files.clear();
    }
}

pub fn detect_backend() -> Result<ClipboardBackend, ClipboardError> {
    Ok(ClipboardBackend::Windows)
}
