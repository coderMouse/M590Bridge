//! Promote local image file paths / file:// URIs into [`ImageClipboard`].

use crate::{ClipboardError, ImageClipboard};
use std::path::{Path, PathBuf};

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];

/// If `text` refers to an existing local image file, decode it to RGBA.
///
/// Accepts:
/// - absolute/relative filesystem paths
/// - `file://` URIs (single or first line)
/// - simple multi-line paste where one line is a path/URI
/// Load the first existing local image among `paths`.
pub fn image_from_paths(
    paths: &[std::path::PathBuf],
) -> Result<Option<ImageClipboard>, ClipboardError> {
    for path in paths {
        if !is_likely_image_path(path) {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        return Ok(Some(load_image_file(path)?));
    }
    Ok(None)
}

pub fn image_from_clipboard_text(text: &str) -> Result<Option<ImageClipboard>, ClipboardError> {
    for candidate in candidate_paths(text) {
        if !is_likely_image_path(&candidate) {
            continue;
        }
        if !candidate.is_file() {
            continue;
        }
        return Ok(Some(load_image_file(&candidate)?));
    }
    Ok(None)
}

/// Decode an image file from disk into RGBA8 [`ImageClipboard`].
pub fn load_image_file(path: &Path) -> Result<ImageClipboard, ClipboardError> {
    let dyn_img = image::open(path)
        .map_err(|e| ClipboardError::Backend(format!("decode image {}: {e}", path.display())))?;
    let rgba = dyn_img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    ImageClipboard::from_rgba(width, height, rgba.into_raw())
}

pub(crate) fn candidate_paths(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return out;
    }

    // Whole blob first (single-line path with spaces).
    if let Some(p) = normalize_path_token(trimmed) {
        out.push(p);
    }

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // GNOME copied-files style noise.
        let lower = line.to_ascii_lowercase();
        if lower == "copy" || lower == "cut" || lower.starts_with("x-special/") {
            continue;
        }
        if let Some(p) = normalize_path_token(line) {
            if !out.iter().any(|e| e == &p) {
                out.push(p);
            }
        }
    }
    out
}

pub(crate) fn normalize_path_token(token: &str) -> Option<PathBuf> {
    let token = token.trim().trim_matches('"').trim_matches('\'');
    if token.is_empty() {
        return None;
    }

    if let Some(rest) = token.strip_prefix("file://") {
        // file:///home/a -> /home/a ; file://localhost/home/a -> /home/a
        let path_part = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode(path_part);
        if decoded.is_empty() {
            return None;
        }
        return Some(PathBuf::from(decoded));
    }

    // Absolute, relative, image-looking, or bare filename (GNOME desktop icon copy).
    let path = PathBuf::from(token);
    if path.is_absolute()
        || token.starts_with('.')
        || is_likely_image_path(&path)
        || is_bare_filename_token(token)
    {
        Some(path)
    } else {
        None
    }
}

fn is_bare_filename_token(token: &str) -> bool {
    if token.is_empty() || token == "." || token == ".." {
        return false;
    }
    if token.contains('/') || token.contains('\\') || token.contains('\0') {
        return false;
    }
    // Prefer tokens that look like files (have a dot) to avoid eating plain sentences.
    token.contains('.')
}

pub(crate) fn is_likely_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

fn percent_decode(input: &str) -> String {
    // Minimal decoder for spaces and non-ascii file names in file:// URIs.
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_png_from_plain_path() {
        let dir = std::env::temp_dir().join(format!("m590-img-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tiny.png");
        // 1x1 red PNG
        let png = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        png.save(&path).unwrap();

        let img = image_from_clipboard_text(path.to_str().unwrap())
            .unwrap()
            .expect("should load");
        assert_eq!((img.width, img.height), (1, 1));
        assert_eq!(img.rgba, vec![255, 0, 0, 255]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_from_file_uri() {
        let dir = std::env::temp_dir().join(format!("m590-img-uri-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("a b.png");
        image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 255, 0, 255]))
            .save(&path)
            .unwrap();
        let uri = format!("file://{}", path.display());
        // insert percent-encoded space variant
        let uri_encoded = uri.replace(' ', "%20");
        let img = image_from_clipboard_text(&uri_encoded)
            .unwrap()
            .expect("uri load");
        assert_eq!(img.rgba[1], 255);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_plain_sentence() {
        assert!(image_from_clipboard_text("hello world").unwrap().is_none());
    }

    #[test]
    fn loads_user_screenshot_if_present() {
        let path = PathBuf::from("/home/huang/图片/截图/截图 2026-07-29 17-19-37.png");
        if !path.is_file() {
            return;
        }
        let img = image_from_clipboard_text(path.to_str().unwrap())
            .unwrap()
            .expect("screenshot should decode");
        assert_eq!((img.width, img.height), (514, 1194));
        assert_eq!(img.rgba.len(), 514 * 1194 * 4);
    }
}
