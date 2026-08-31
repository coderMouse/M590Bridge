//! Platform clipboard abstraction.
//!
//! - Linux (task-004/014): text + image read / write / poll-watch via `arboard`
//! - Windows (task-005/014): same API surface via `arboard` + Win32
//!
//! Linux strategy (Q4):
//! - Prefer Wayland when `WAYLAND_DISPLAY` is set
//! - Else X11 when `DISPLAY` is set
//! - Else report `ClipboardError::NoDisplay`
//!
//! Windows strategy:
//! - Backend label is always [`ClipboardBackend::Windows`]
//! - Open failures surface as [`ClipboardError::Backend`] / [`ClipboardError::NoDisplay`]

use std::io::Cursor;

use image::ImageDecoder;

mod error;
mod file_paths;
mod image_file;
mod virtual_file;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod arboard_text;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
mod windows_virtual_file;

pub use error::ClipboardError;
pub use file_paths::{
    first_regular_file, local_paths_from_text, read_file_for_offer, regular_file_from_text,
};
pub use image_file::{
    image_from_clipboard_text, image_from_paths, is_decodable_image_path, is_likely_image_path,
    load_image_file,
};
pub use virtual_file::{VirtualFile, VirtualFileCollection, VirtualFileCollectionEntry};

#[cfg(target_os = "windows")]
pub use windows_virtual_file::{
    publish_virtual_file, publish_virtual_file_collection, pump_virtual_file_messages,
    VirtualFileClipboard,
};

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

/// Image clipboard payload (raw RGBA8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageClipboard {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImageClipboard {
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ClipboardError> {
        if width == 0 || height == 0 {
            return Err(ClipboardError::Backend(
                "image dimensions must be non-zero".into(),
            ));
        }
        validate_image_dimensions(width, height)?;
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| ClipboardError::Backend("image dimensions overflow".into()))?;
        if rgba.len() != expected {
            return Err(ClipboardError::Backend(format!(
                "rgba length mismatch: got {} expected {expected}",
                rgba.len()
            )));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub fn fingerprint(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.width.hash(&mut hasher);
        self.height.hash(&mut hasher);
        self.rgba.hash(&mut hasher);
        hasher.finish()
    }

    /// Encode as PNG (for compact on-wire transfer).
    pub fn to_png_bytes(&self) -> Result<Vec<u8>, ClipboardError> {
        let img = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .ok_or_else(|| ClipboardError::Backend("rgba buffer rejected by image crate".into()))?;
        let mut out = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        use image::ImageEncoder;
        encoder
            .write_image(
                img.as_raw(),
                self.width,
                self.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| ClipboardError::Backend(format!("png encode: {e}")))?;
        Ok(out)
    }

    /// Windows clipboard `CF_DIB` payload (40-byte `BITMAPINFOHEADER` + bottom-up
    /// BGRA), the classic bitmap format every bitmap consumer understands.
    ///
    /// task-061 real-machine traces: consumers issued ~25 `GetData` calls per
    /// paste (text, HDROP, a range of registered formats) but **not one** for
    /// `CF_BITMAP` (2), `CF_DIB` (8) or the `CF_DIBV5` (17) we did advertise. So
    /// the data object offered no bitmap format anyone came to fetch. Whether
    /// `CF_DIB` alone makes Word's paste work is unconfirmed — it needs a real
    /// Windows machine.
    ///
    /// `BI_RGB` with 32 bpp: the alpha byte is ignored by consumers rather than
    /// premultiplied, which matches how `CF_DIB` is conventionally produced.
    pub fn to_dib_bytes(&self) -> Result<Vec<u8>, ClipboardError> {
        const BI_RGB: u32 = 0;
        const HEADER_BYTES: usize = 40;
        let size_image_u32 = u32::try_from((self.width as u64) * (self.height as u64) * 4)
            .map_err(|_| ClipboardError::Backend("image too large for DIB".into()))?;
        let mut out = Vec::with_capacity(HEADER_BYTES + size_image_u32 as usize);
        out.extend_from_slice(&(HEADER_BYTES as u32).to_le_bytes()); // biSize
        out.extend_from_slice(&(self.width as i32).to_le_bytes());
        // Positive height: bottom-up rows, same convention as our DIBv5 payload.
        out.extend_from_slice(&(self.height as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
        out.extend_from_slice(&BI_RGB.to_le_bytes());
        out.extend_from_slice(&size_image_u32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
        out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
        out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
        out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
        debug_assert_eq!(out.len(), HEADER_BYTES);
        self.append_bottom_up_bgra(&mut out);
        Ok(out)
    }

    /// Bottom-up rows of BGRA pixels, the shared pixel layout of `CF_DIB` and
    /// `CF_DIBV5`.
    fn append_bottom_up_bgra(&self, out: &mut Vec<u8>) {
        for row in (0..self.height).rev() {
            for col in 0..self.width {
                let index = ((row * self.width + col) * 4) as usize;
                // RGBA -> DIB BGRA.
                out.extend_from_slice(&[
                    self.rgba[index + 2],
                    self.rgba[index + 1],
                    self.rgba[index],
                    self.rgba[index + 3],
                ]);
            }
        }
    }

    /// Windows clipboard DIBv5 payload (`BITMAPV5HEADER` + bottom-up BGRA).
    ///
    /// Byte-layout mirrors what `arboard` writes on Windows (`CF_DIBV5`,
    /// positive height, `BI_BITFIELDS` with RGBA masks, `LCS_sRGB`), so serving
    /// this from an OLE data object yields the same pixels Word/WordPad read
    /// and the same fingerprint our poll sees (no sync echo).
    pub fn to_dibv5_bytes(&self) -> Result<Vec<u8>, ClipboardError> {
        const LCS_SRGB: u32 = 0x7352_4742;
        const LCS_GM_IMAGES: u32 = 4;
        const BI_BITFIELDS: u32 = 3;
        let size_image_u32 = u32::try_from((self.width as u64) * (self.height as u64) * 4)
            .map_err(|_| ClipboardError::Backend("image too large for DIBv5".into()))?;
        let mut out = Vec::with_capacity(124 + size_image_u32 as usize);
        out.extend_from_slice(&124u32.to_le_bytes()); // bV5Size
        out.extend_from_slice(&(self.width as i32).to_le_bytes());
        // Positive height: bottom-up rows (Word cannot paste negative-height DIBs).
        out.extend_from_slice(&(self.height as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit count
        out.extend_from_slice(&BI_BITFIELDS.to_le_bytes());
        out.extend_from_slice(&size_image_u32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes()); // xPelsPerMeter
        out.extend_from_slice(&0i32.to_le_bytes()); // yPelsPerMeter
        out.extend_from_slice(&0u32.to_le_bytes()); // clrUsed
        out.extend_from_slice(&0u32.to_le_bytes()); // clrImportant
        out.extend_from_slice(&0x00ff_0000u32.to_le_bytes()); // red mask
        out.extend_from_slice(&0x0000_ff00u32.to_le_bytes()); // green mask
        out.extend_from_slice(&0x0000_00ffu32.to_le_bytes()); // blue mask
        out.extend_from_slice(&0xff00_0000u32.to_le_bytes()); // alpha mask
        out.extend_from_slice(&LCS_SRGB.to_le_bytes());
        out.extend_from_slice(&[0u8; 36]); // bV5Endpoints
        out.extend_from_slice(&0u32.to_le_bytes()); // gammaRed
        out.extend_from_slice(&0u32.to_le_bytes()); // gammaGreen
        out.extend_from_slice(&0u32.to_le_bytes()); // gammaBlue
        out.extend_from_slice(&LCS_GM_IMAGES.to_le_bytes()); // intent
        out.extend_from_slice(&0u32.to_le_bytes()); // profileData
        out.extend_from_slice(&0u32.to_le_bytes()); // profileSize
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        debug_assert_eq!(out.len(), 124);
        self.append_bottom_up_bgra(&mut out);
        Ok(out)
    }

    /// Decode wire payload (raw RGBA or PNG) into an OS-clipboard image.
    pub fn from_wire(
        width: u32,
        height: u32,
        encoding: m590_core::ImageEncoding,
        data: Vec<u8>,
    ) -> Result<Self, ClipboardError> {
        validate_image_dimensions(width, height)?;
        match encoding {
            m590_core::ImageEncoding::RawRgba => Self::from_rgba(width, height, data),
            m590_core::ImageEncoding::Png => {
                // Read PNG metadata with the decoder limits before allocating the decoded image.
                let metadata_decoder =
                    image::codecs::png::PngDecoder::new(Cursor::new(data.as_slice()))
                        .map_err(|e| ClipboardError::Backend(format!("png metadata: {e}")))?;
                let (decoded_width, decoded_height) = metadata_decoder.dimensions();
                validate_image_dimensions(decoded_width, decoded_height)?;

                let mut reader = image::ImageReader::with_format(
                    Cursor::new(data.as_slice()),
                    image::ImageFormat::Png,
                );
                reader.limits(image_decode_limits());
                let dyn_img = reader
                    .decode()
                    .map_err(|e| ClipboardError::Backend(format!("png decode: {e}")))?;
                let rgba = dyn_img.to_rgba8();
                validate_image_dimensions(rgba.width(), rgba.height())?;
                Self::from_rgba(rgba.width(), rgba.height(), rgba.into_raw())
            }
        }
    }

    /// Choose PNG when it fits the inline budget (typical screenshots); else raw if it fits.
    pub fn prepare_inline(
        &self,
        max_bytes: usize,
    ) -> Result<(m590_core::ImageEncoding, Vec<u8>), ClipboardError> {
        match self.to_png_bytes() {
            Ok(png) if png.len() <= max_bytes => Ok((m590_core::ImageEncoding::Png, png)),
            Ok(png) if self.rgba.len() <= max_bytes => {
                // PNG larger than budget somehow but raw fits — rare.
                let _ = png;
                Ok((m590_core::ImageEncoding::RawRgba, self.rgba.clone()))
            }
            Ok(png) => Err(ClipboardError::Backend(format!(
                "image too large for inline sync png={}B raw={}B limit={max_bytes}B",
                png.len(),
                self.rgba.len()
            ))),
            Err(_png_err) if self.rgba.len() <= max_bytes => {
                Ok((m590_core::ImageEncoding::RawRgba, self.rgba.clone()))
            }
            Err(e) => Err(e),
        }
    }
}

fn validate_image_dimensions(width: u32, height: u32) -> Result<(), ClipboardError> {
    let pixels = (width as u64)
        .checked_mul(height as u64)
        .ok_or_else(|| ClipboardError::Backend("image dimensions overflow".into()))?;
    if pixels > m590_core::MAX_IMAGE_PIXELS {
        return Err(ClipboardError::Backend(format!(
            "image dimensions exceed pixel limit: {pixels} > {}",
            m590_core::MAX_IMAGE_PIXELS
        )));
    }
    Ok(())
}

fn image_decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(m590_core::MAX_IMAGE_PIXELS.saturating_mul(4));
    limits
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

    /// Read current image. `Ok(None)` means no image content.
    fn read_image(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        let _ = self;
        Ok(None)
    }

    /// Replace clipboard image.
    fn write_image(&mut self, image: &ImageClipboard) -> Result<(), ClipboardError> {
        let _ = image;
        Err(ClipboardError::UnsupportedPlatform)
    }

    /// Poll for image change since open / last poll / last local write.
    fn poll_image_change(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        let _ = self;
        Ok(None)
    }

    /// Read file paths currently offered by the clipboard (file manager copy).
    fn read_file_list(&mut self) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        let _ = self;
        Ok(Vec::new())
    }

    /// Replace the clipboard with local file paths.
    fn write_file_list(&mut self, paths: &[std::path::PathBuf]) -> Result<(), ClipboardError> {
        let _ = paths;
        Err(ClipboardError::UnsupportedPlatform)
    }

    /// Poll for file-list change since open / last poll.
    ///
    /// Returns `Ok(Some(paths))` when the list changed (including to empty).
    /// Callers typically care about non-empty image paths.
    fn poll_file_list_change(&mut self) -> Result<Option<Vec<std::path::PathBuf>>, ClipboardError> {
        let _ = self;
        Ok(None)
    }

    /// Drop poll baselines so the next poll reports the *current* clipboard once.
    ///
    /// Useful right after a session becomes Connected (baselines may have been
    /// captured during pairing and would otherwise hide an already-copied image).
    fn prime_poll_to_emit_current(&mut self) {
        let _ = self;
    }

    /// Re-arm only file-list and path-text polling for the current clipboard.
    ///
    /// A streamed virtual file is single-use. After it completes or is cancelled,
    /// the same local clipboard file needs to produce a fresh transfer without
    /// re-emitting an unchanged bitmap.
    fn rearm_file_offer_poll(&mut self) {
        let _ = self;
    }

    /// Set text poll baseline to whatever is currently on the clipboard (no emit).
    ///
    /// Used after a file_list / path-text file offer so the path string is not
    /// also pushed as ClipboardText.
    fn adopt_text_baseline(&mut self) {
        let _ = self;
    }

    /// Record `image` as the current clipboard image baseline without writing.
    ///
    /// Used when an application-specific representation (e.g. a Windows OLE
    /// virtual image offer that also serves the bitmap formats) replaces the
    /// bitmap on the clipboard instead of [`Self::write_image`].
    fn adopt_image_baseline(&mut self, image: &ImageClipboard) {
        let _ = image;
    }
}

/// No-op clipboard used in tests and headless demos.
#[derive(Debug, Default)]
pub struct NullClipboard {
    text: Option<String>,
    last_seen: Option<String>,
    image: Option<ImageClipboard>,
    last_image_fp: Option<u64>,
    files: Vec<std::path::PathBuf>,
    last_files: Vec<std::path::PathBuf>,
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

    fn read_image(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        Ok(self.image.clone())
    }

    fn write_image(&mut self, image: &ImageClipboard) -> Result<(), ClipboardError> {
        self.image = Some(image.clone());
        self.last_image_fp = Some(image.fingerprint());
        Ok(())
    }

    fn poll_image_change(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        let fp = self.image.as_ref().map(|img| img.fingerprint());
        if fp != self.last_image_fp {
            self.last_image_fp = fp;
            Ok(self.image.clone())
        } else {
            Ok(None)
        }
    }

    fn read_file_list(&mut self) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        Ok(self.files.clone())
    }

    fn write_file_list(&mut self, paths: &[std::path::PathBuf]) -> Result<(), ClipboardError> {
        self.files = paths.to_vec();
        self.last_files = self.files.clone();
        Ok(())
    }

    fn poll_file_list_change(&mut self) -> Result<Option<Vec<std::path::PathBuf>>, ClipboardError> {
        if self.files != self.last_files {
            self.last_files = self.files.clone();
            Ok(Some(self.files.clone()))
        } else {
            Ok(None)
        }
    }

    fn prime_poll_to_emit_current(&mut self) {
        self.last_seen = None;
        self.last_image_fp = None;
        self.last_files.clear();
    }

    fn rearm_file_offer_poll(&mut self) {
        self.last_seen = None;
        self.last_files.clear();
    }

    fn adopt_text_baseline(&mut self) {
        self.last_seen = self.text.clone();
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

    fn read_image(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.read_image()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn write_image(&mut self, image: &ImageClipboard) -> Result<(), ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.write_image(image)
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = image;
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn poll_image_change(&mut self) -> Result<Option<ImageClipboard>, ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.poll_image_change()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn read_file_list(&mut self) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.read_file_list()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn write_file_list(&mut self, paths: &[std::path::PathBuf]) -> Result<(), ClipboardError> {
        #[cfg(target_os = "linux")]
        {
            self.inner.write_file_list(paths)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = paths;
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn poll_file_list_change(&mut self) -> Result<Option<Vec<std::path::PathBuf>>, ClipboardError> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.poll_file_list_change()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(ClipboardError::UnsupportedPlatform)
        }
    }

    fn prime_poll_to_emit_current(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.prime_poll_to_emit_current();
        }
    }

    fn rearm_file_offer_poll(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.rearm_file_offer_poll();
        }
    }

    fn adopt_text_baseline(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.adopt_text_baseline();
        }
    }

    fn adopt_image_baseline(&mut self, image: &ImageClipboard) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.inner.adopt_image_baseline(image);
        }
    }
}

/// Whether background file-list watch is likely to see file-manager copies.
///
/// On Linux Wayland without ext/wlr-data-control, `arboard` falls back to X11 and
/// typically **cannot** observe Nautilus/GNOME file copies (uri-list stays on Wayland).
/// Text may still work via X11 bridging. UI should offer pick/drag send instead.
pub fn file_clipboard_watch_likely() -> bool {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        if !wayland {
            return true;
        }
        // Same gate arboard uses before selecting the Wayland data-control backend.
        // Only Ok(_) means ext/wlr-data-control is present; any Err → cannot watch.
        wl_clipboard_rs::utils::is_primary_selection_supported().is_ok()
    }
    #[cfg(target_os = "windows")]
    {
        true
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
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
        assert_eq!(
            clip.poll_text_change().unwrap().as_deref(),
            Some("external")
        );
        assert_eq!(clip.poll_text_change().unwrap(), None);

        let img = ImageClipboard::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        clip.write_image(&img).unwrap();
        assert_eq!(clip.read_image().unwrap(), Some(img.clone()));
        assert_eq!(clip.poll_image_change().unwrap(), None);
        let img2 = ImageClipboard::from_rgba(1, 1, vec![9, 8, 7, 255]).unwrap();
        clip.image = Some(img2.clone());
        assert_eq!(clip.poll_image_change().unwrap(), Some(img2));

        let written_files = vec![std::path::PathBuf::from("/tmp/written.txt")];
        clip.write_file_list(&written_files).unwrap();
        assert_eq!(clip.read_file_list().unwrap(), written_files);
        assert_eq!(clip.poll_file_list_change().unwrap(), None);

        clip.files = vec![std::path::PathBuf::from("/tmp/a.png")];
        assert_eq!(
            clip.poll_file_list_change().unwrap(),
            Some(vec![std::path::PathBuf::from("/tmp/a.png")])
        );
        assert_eq!(clip.poll_file_list_change().unwrap(), None);

        clip.rearm_file_offer_poll();
        assert_eq!(
            clip.poll_file_list_change().unwrap(),
            Some(vec![std::path::PathBuf::from("/tmp/a.png")])
        );
        assert_eq!(clip.poll_image_change().unwrap(), None);
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
    fn dibv5_layout_matches_windows_bitmap_consumer() {
        // The payload is served as Windows `CF_DIBV5`; Word/WordPad read the
        // BITMAPV5HEADER at fixed offsets and take the bits at `bV5Size`, so we
        // assert the standard layout byte-for-byte instead of round-tripping
        // through the `image` crate's BMP decoder (which since 0.25.7 assumes
        // an extra 12-byte mask residue between the V5 header and the bits).
        const BI_BITFIELDS: u32 = 3;
        const LCS_SRGB: u32 = 0x7352_4742;
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 64, 32, 16,
        ];
        let image = ImageClipboard::from_rgba(2, 2, rgba.clone()).unwrap();
        let dib = image.to_dibv5_bytes().unwrap();

        let u16_at = |off: usize| u16::from_le_bytes(dib[off..off + 2].try_into().unwrap());
        let i32_at = |off: usize| i32::from_le_bytes(dib[off..off + 4].try_into().unwrap());
        let u32_at = |off: usize| u32::from_le_bytes(dib[off..off + 4].try_into().unwrap());
        assert_eq!(u32_at(0), 124); // bV5Size
        assert_eq!(i32_at(4), 2); // width
        assert_eq!(i32_at(8), 2); // positive height: bottom-up rows (Word rejects negative heights)
        assert_eq!(u16_at(12), 1); // planes
        assert_eq!(u16_at(14), 32); // bit count
        assert_eq!(u32_at(16), BI_BITFIELDS);
        assert_eq!(u32_at(20), 2 * 2 * 4); // bV5SizeImage
        assert_eq!(u32_at(40), 0x00ff_0000); // red mask
        assert_eq!(u32_at(44), 0x0000_ff00); // green mask
        assert_eq!(u32_at(48), 0x0000_00ff); // blue mask
        assert_eq!(u32_at(52), 0xff00_0000); // alpha mask
        assert_eq!(u32_at(56), LCS_SRGB);
        assert_eq!(dib.len(), 124 + 16);

        // Bits at offset bV5Size: BGRA byte order, bottom-up rows (this is the
        // byte layout arboard writes from its `flip_v` + `rgba_to_win` helpers).
        // Rows of `rgba` are top-down: red, green / blue, teal.
        let expected = [
            255, 0, 0, 255, // blue row -> bottom row
            32, 64, 128, 16, // teal
            0, 0, 255, 255, // red row -> top row
            0, 255, 0, 255, // green
        ];
        assert_eq!(&dib[124..], &expected);

        // Independent consumer-side read: header at fixed offsets, bits at
        // bV5Size, BGRA -> RGBA, bottom-up -> top-down. Round-trips to the
        // original RGBA image.
        let width = i32_at(4) as usize;
        let height = i32_at(8) as usize;
        let bits = &dib[u32_at(0) as usize..];
        let mut decoded = vec![0u8; width * height * 4];
        for row in 0..height {
            for col in 0..width {
                let src = ((height - 1 - row) * width + col) * 4;
                let dst = (row * width + col) * 4;
                decoded[dst..dst + 3].copy_from_slice(&bits[src..src + 3]);
                decoded[dst..dst + 3].reverse(); // BGRA -> RGB
                decoded[dst + 3] = bits[src + 3];
            }
        }
        assert_eq!(decoded, rgba);
    }

    #[test]
    fn dib_layout_matches_windows_bitmap_consumer() {
        // task-061: no consumer requested any bitmap format in the real-machine
        // trace (not CF_DIB, CF_BITMAP, nor the CF_DIBV5 we advertised), so the
        // same pixels are now also served as plain CF_DIB. 40-byte
        // BITMAPINFOHEADER, BI_RGB, 32 bpp, bits immediately after the header.
        const BI_RGB: u32 = 0;
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 64, 32, 16,
        ];
        let image = ImageClipboard::from_rgba(2, 2, rgba.clone()).unwrap();
        let dib = image.to_dib_bytes().unwrap();

        let u16_at = |off: usize| u16::from_le_bytes(dib[off..off + 2].try_into().unwrap());
        let i32_at = |off: usize| i32::from_le_bytes(dib[off..off + 4].try_into().unwrap());
        let u32_at = |off: usize| u32::from_le_bytes(dib[off..off + 4].try_into().unwrap());
        assert_eq!(u32_at(0), 40); // biSize
        assert_eq!(i32_at(4), 2); // width
        assert_eq!(i32_at(8), 2); // positive height: bottom-up rows
        assert_eq!(u16_at(12), 1); // biPlanes
        assert_eq!(u16_at(14), 32); // biBitCount
        assert_eq!(u32_at(16), BI_RGB);
        assert_eq!(u32_at(20), 2 * 2 * 4); // biSizeImage
        assert_eq!(dib.len(), 40 + 16);

        // Same pixel block as the DIBv5 payload: BGRA, bottom-up.
        assert_eq!(&dib[40..], &image.to_dibv5_bytes().unwrap()[124..]);
    }

    #[test]
    fn prepare_inline_prefers_png_and_roundtrips() {
        // Smooth image compresses well vs raw RGBA.
        let mut rgba = vec![0u8; 200 * 100 * 4];
        for px in rgba.chunks_mut(4) {
            px[0] = 10;
            px[1] = 20;
            px[2] = 30;
            px[3] = 255;
        }
        let img = ImageClipboard::from_rgba(200, 100, rgba).unwrap();
        let raw_len = img.rgba.len();
        let (enc, data) = img.prepare_inline(12 * 1024 * 1024).unwrap();
        assert_eq!(enc, m590_core::ImageEncoding::Png);
        assert!(data.len() < raw_len);
        let back = ImageClipboard::from_wire(200, 100, enc, data).unwrap();
        assert_eq!((back.width, back.height), (200, 100));
        assert_eq!(back.rgba.len(), raw_len);
    }

    #[test]
    fn rejects_png_with_excessive_decoded_dimensions() {
        let image = ImageClipboard::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let (_, mut png) = image.prepare_inline(12 * 1024 * 1024).unwrap();

        // Change the IHDR dimensions while keeping the test fixture structurally valid.
        let width = m590_core::MAX_IMAGE_PIXELS as u32 + 1;
        png[16..20].copy_from_slice(&width.to_be_bytes());
        png[20..24].copy_from_slice(&1u32.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());

        let err = ImageClipboard::from_wire(1, 1, m590_core::ImageEncoding::Png, png).unwrap_err();
        assert!(err.to_string().contains("pixel limit"), "{err}");
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb88320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
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
