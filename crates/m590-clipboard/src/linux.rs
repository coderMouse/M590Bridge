//! Linux text/image clipboard via `arboard`.
//!
//! Detection order:
//! 1. `WAYLAND_DISPLAY` → [`ClipboardBackend::Wayland`]
//! 2. `DISPLAY` → [`ClipboardBackend::X11`]
//! 3. else [`ClipboardError::NoDisplay`]

use crate::arboard_text::{
    open_clipboard, read_file_list_raw, read_image_raw, read_text_raw, write_image_raw,
    write_text_raw,
};
use crate::{
    file_paths::paths_from_file_list_text, ClipboardBackend, ClipboardError, ClipboardService,
    ImageClipboard,
};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const WAYLAND_FILE_LIST_PROBE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug)]
struct WaylandFileListProbe {
    arboard_paths: Vec<PathBuf>,
    best_paths: Vec<PathBuf>,
    checked_at: Instant,
}

pub struct LinuxClipboard {
    backend: ClipboardBackend,
    clipboard: arboard::Clipboard,
    last_seen: Option<String>,
    last_image_fp: Option<u64>,
    last_files: Vec<std::path::PathBuf>,
    wayland_file_probe: Option<WaylandFileListProbe>,
}

impl std::fmt::Debug for LinuxClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxClipboard")
            .field("backend", &self.backend)
            .field("last_seen", &self.last_seen)
            .field("last_image_fp", &self.last_image_fp)
            .field("last_files", &self.last_files)
            .field("wayland_file_probe", &self.wayland_file_probe)
            .finish_non_exhaustive()
    }
}

impl LinuxClipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        let backend = detect_backend()?;
        let mut clipboard = open_clipboard()?;
        let last_seen = read_text_raw(&mut clipboard)?;
        let last_image_fp = read_image_raw(&mut clipboard)?
            .as_ref()
            .map(|img| img.fingerprint());
        let last_files = read_file_list_raw(&mut clipboard).unwrap_or_default();
        Ok(Self {
            backend,
            clipboard,
            last_seen,
            last_image_fp,
            last_files,
            wayland_file_probe: None,
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

    fn read_file_list(&mut self) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        self.read_file_list_current()
    }

    fn write_file_list(&mut self, paths: &[std::path::PathBuf]) -> Result<(), ClipboardError> {
        self.clipboard
            .set()
            .file_list(paths)
            .map_err(|err| ClipboardError::Backend(err.to_string()))?;
        self.last_files = paths.to_vec();
        self.wayland_file_probe = None;
        Ok(())
    }

    fn poll_file_list_change(&mut self) -> Result<Option<Vec<std::path::PathBuf>>, ClipboardError> {
        let current = self.read_file_list_current()?;
        if current != self.last_files {
            self.last_files = current.clone();
            Ok(Some(current))
        } else {
            Ok(None)
        }
    }

    fn prime_poll_to_emit_current(&mut self) {
        self.last_seen = None;
        self.last_image_fp = None;
        self.last_files.clear();
        self.wayland_file_probe = None;
    }

    fn rearm_file_offer_poll(&mut self) {
        self.last_seen = None;
        self.last_files.clear();
        self.wayland_file_probe = None;
    }

    fn adopt_text_baseline(&mut self) {
        self.last_seen = read_text_raw(&mut self.clipboard).ok().flatten();
    }
}

impl LinuxClipboard {
    fn read_file_list_current(&mut self) -> Result<Vec<PathBuf>, ClipboardError> {
        let arboard_paths = read_file_list_raw(&mut self.clipboard)?;
        if arboard_paths.len() > 1 || std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Ok(arboard_paths);
        }

        if let Some(probe) = self.wayland_file_probe.as_ref() {
            if probe.arboard_paths == arboard_paths
                && probe.checked_at.elapsed() < WAYLAND_FILE_LIST_PROBE_INTERVAL
            {
                return Ok(probe.best_paths.clone());
            }
        }

        // Some GNOME/Nautilus producers expose the complete selection through a
        // Wayland-only MIME offer while arboard's selected backend sees only the
        // first URI (or no URI at all). Ask the compositor directly for both
        // standard MIME forms and prefer a larger complete list when available.
        let best_paths = read_wayland_file_list()
            .filter(|paths| paths.len() > arboard_paths.len())
            .unwrap_or_else(|| arboard_paths.clone());
        self.wayland_file_probe = Some(WaylandFileListProbe {
            arboard_paths,
            best_paths: best_paths.clone(),
            checked_at: Instant::now(),
        });
        Ok(best_paths)
    }
}

fn read_wayland_file_list() -> Option<Vec<PathBuf>> {
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};

    let mut best = None;
    for mime in ["text/uri-list", "x-special/gnome-copied-files"] {
        let Ok((mut pipe, _actual_mime)) = get_contents(
            ClipboardType::Regular,
            Seat::Unspecified,
            MimeType::Specific(mime),
        ) else {
            continue;
        };
        let mut bytes = Vec::new();
        if pipe.read_to_end(&mut bytes).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let paths = paths_from_file_list_text(&text);
        if paths.is_empty() {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|current: &Vec<PathBuf>| paths.len() > current.len())
        {
            best = Some(paths);
        }
    }
    best
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
