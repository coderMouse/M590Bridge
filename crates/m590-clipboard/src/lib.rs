//! Platform clipboard abstraction.
//!
//! - Linux (task-004): text read / write / poll-watch via `arboard`
//! - Windows (task-005): same API surface via `arboard` + Win32
//!
//! Linux strategy (Q4):
//! - Prefer Wayland when `WAYLAND_DISPLAY` is set
//! - Else X11 when `DISPLAY` is set
//! - Else report `ClipboardError::NoDisplay`
//!
//! Windows strategy:
//! - Backend label is always [`ClipboardBackend::Windows`]
//! - Open failures surface as [`ClipboardError::Backend`] / [`ClipboardError::NoDisplay`]

mod error;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod arboard_text;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

pub use error::ClipboardError;

/// Which OS/display backend is selected or available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardBackend {
    /// Uninitialized / no-op.
    Unspecified,
    /// Linux X11 (`DISPLAY`).
    #[cfg(target_os = "linux")]
    X11,
    /// Linux Wayland (`WAYLAND_DISPLAY`).
    #[cfg(target_os = "linux")]
    Wayland,
    /// Windows Win32 clipboard.
    #[cfg(target_os = "windows")]
    Windows,
}

/// Text clipboard payload (MVP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextClipboard {
    pub text: String,
}

impl TextClipboard {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Trait boundary for clipboard backends.
pub trait ClipboardService {
    fn backend(&self) -> ClipboardBackend;

    /// Read current text. `Ok(None)` means empty / non-text content.
    fn read_text(&mut self) -> Result<Option<String>, ClipboardError>;

    /// Replace clipboard text.
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;

    /// Poll for text change since open / last poll.
    ///
    /// Returns `Ok(Some(text))` when content changed, `Ok(None)` if unchanged.
    /// First baseline is captured on `open` (and refreshed after successful writes
    /// from this handle when using [`PlatformClipboard`]).
    fn poll_text_change(&mut self) -> Result<Option<String>, ClipboardError>;
}

/// No-op clipboard used in tests and headless demos.
#[derive(Debug, Default)]
pub struct NullClipboard {
    text: Option<String>,
    last_seen: Option<String>,
}

impl NullClipboard {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ClipboardService for NullClipboard {
    fn backend(&self) -> ClipboardBackend {
        ClipboardBackend::Unspecified
    }

    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(self.text.clone())
    }

    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.text = Some(text.to_string());
        self.last_seen = self.text.clone();
        Ok(())
    }

    fn poll_text_change(&mut self) -> Result<Option<String>, ClipboardError> {
        if self.text != self.last_seen {
            self.last_seen = self.text.clone();
            Ok(self.text.clone())
        } else {
            Ok(None)
        }
    }
}

/// Platform clipboard handle.
#[derive(Debug)]
pub struct PlatformClipboard {
    #[cfg(target_os = "linux")]
    inner: linux::LinuxClipboard,
    #[cfg(target_os = "windows")]
    inner: windows::WindowsClipboard,
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    _private: (),
}

impl PlatformClipboard {
    /// Open the platform clipboard.
    pub fn open() -> Result<Self, ClipboardError> {
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                inner: linux::LinuxClipboard::open()?,
            })
        }
        #[cfg(target_os = "windows")]
        {
            Ok(Self {
                inner: windows::WindowsClipboard::open()?,
            })
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    /// Detect preferred backend without opening the clipboard.
    pub fn detect_backend() -> Result<ClipboardBackend, ClipboardError> {
        #[cfg(target_os = "linux")]
        {
            linux::detect_backend()
        }
        #[cfg(target_os = "windows")]
        {
            windows::detect_backend()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }
}

impl ClipboardService for PlatformClipboard {
    fn backend(&self) -> ClipboardBackend {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.backend()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            ClipboardBackend::Unspecified
        }
    }

    fn read_text(&mut self) -> Result<Option<String>, ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.read_text()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.write_text(text)
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = text;
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn poll_text_change(&mut self) -> Result<Option<String>, ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.poll_text_change()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }
}

/// Backends compiled into this build (not necessarily usable at runtime).
pub fn available_backends() -> Vec<ClipboardBackend> {
    let mut backends = vec![ClipboardBackend::Unspecified];
    #[cfg(target_os = "linux")]
    {
        backends.push(ClipboardBackend::X11);
        backends.push(ClipboardBackend::Wayland);
    }
    #[cfg(target_os = "windows")]
    {
        backends.push(ClipboardBackend::Windows);
    }
    backends
}

/// Owner label helper for future clipboard managers.
pub fn placeholder_owner_label() -> String {
    format!("{}-clipboard", m590_core::APP_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_clipboard_roundtrip_and_poll() {
        let mut clip = NullClipboard::new();
        assert_eq!(clip.backend(), ClipboardBackend::Unspecified);
        assert_eq!(clip.read_text().unwrap(), None);
        clip.write_text("hello").unwrap();
        assert_eq!(clip.read_text().unwrap().as_deref(), Some("hello"));
        assert_eq!(clip.poll_text_change().unwrap(), None);
        clip.text = Some("external".into());
        assert_eq!(clip.poll_text_change().unwrap().as_deref(), Some("external"));
        assert_eq!(clip.poll_text_change().unwrap(), None);
    }

    #[test]
    fn available_backends_includes_unspecified() {
        assert!(available_backends().contains(&ClipboardBackend::Unspecified));
    }

    #[test]
    fn owner_label_uses_app_name() {
        assert!(placeholder_owner_label().contains(m590_core::APP_NAME));
    }

    #[test]
    fn platform_detect_or_open_does_not_lie_without_display() {
        match PlatformClipboard::detect_backend() {
            Ok(backend) => {
                #[cfg(target_os = "linux")]
                assert!(matches!(
                    backend,
                    ClipboardBackend::X11 | ClipboardBackend::Wayland
                ));
                #[cfg(target_os = "windows")]
                assert_eq!(backend, ClipboardBackend::Windows);
                let _ = backend;
            }
            Err(err) => {
                assert!(matches!(
                    err,
                    ClipboardError::NoDisplay | ClipboardError::UnsupportedPlatform
                ));
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_text_write_read_poll_if_clipboard_available() {
        let mut clip = match PlatformClipboard::open() {
            Ok(c) => c,
            Err(err) => {
                eprintln!("skip linux clipboard integration: {err}");
                return;
            }
        };

        let marker = format!(
            "m590-clipboard-test-{}-{}",
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

        let marker2 = format!("{marker}-changed");
        clip.write_text(&marker2).unwrap();
        assert_eq!(clip.poll_text_change().unwrap(), None);

        if let Ok(mut other) = PlatformClipboard::open() {
            let external = format!("{marker}-external");
            other.write_text(&external).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let changed = clip.poll_text_change().unwrap();
            assert_eq!(changed.as_deref(), Some(external.as_str()));
        }
    }
}
