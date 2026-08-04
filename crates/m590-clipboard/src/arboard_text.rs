//! Shared text/image helpers on top of `arboard` (Linux / Windows).

use crate::{ClipboardError, ImageClipboard};
use std::borrow::Cow;

pub(crate) fn open_clipboard() -> Result<arboard::Clipboard, ClipboardError> {
    arboard::Clipboard::new().map_err(|e| ClipboardError::Backend(e.to_string()))
}

pub(crate) fn read_text_raw(
    clipboard: &mut arboard::Clipboard,
) -> Result<Option<String>, ClipboardError> {
    match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(arboard::Error::ClipboardNotSupported) => Err(ClipboardError::NoDisplay),
        Err(other) => {
            let msg = other.to_string();
            let lower = msg.to_lowercase();
            if lower.contains("not available") || lower.contains("empty") {
                Ok(None)
            } else {
                Err(ClipboardError::Backend(msg))
            }
        }
    }
}

pub(crate) fn write_text_raw(
    clipboard: &mut arboard::Clipboard,
    text: &str,
) -> Result<(), ClipboardError> {
    clipboard
        .set_text(text.to_string())
        .map_err(|e| ClipboardError::Backend(e.to_string()))
}

pub(crate) fn read_image_raw(
    clipboard: &mut arboard::Clipboard,
) -> Result<Option<ImageClipboard>, ClipboardError> {
    match clipboard.get_image() {
        Ok(img) => {
            let width = img.width as u32;
            let height = img.height as u32;
            let rgba = img.bytes.into_owned();
            match ImageClipboard::from_rgba(width, height, rgba) {
                Ok(image) => Ok(Some(image)),
                Err(err) => Err(ClipboardError::Backend(err.to_string())),
            }
        }
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(arboard::Error::ClipboardNotSupported) => Err(ClipboardError::NoDisplay),
        Err(other) => {
            let msg = other.to_string();
            let lower = msg.to_lowercase();
            if lower.contains("not available") || lower.contains("empty") {
                Ok(None)
            } else {
                Err(ClipboardError::Backend(msg))
            }
        }
    }
}

pub(crate) fn write_image_raw(
    clipboard: &mut arboard::Clipboard,
    image: &ImageClipboard,
) -> Result<(), ClipboardError> {
    let data = arboard::ImageData {
        width: image.width as usize,
        height: image.height as usize,
        bytes: Cow::Borrowed(image.rgba.as_slice()),
    };
    clipboard
        .set_image(data)
        .map_err(|e| ClipboardError::Backend(e.to_string()))
}
