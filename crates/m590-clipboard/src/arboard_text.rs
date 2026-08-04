//! Shared text helpers on top of `arboard` (Linux / Windows).

use crate::ClipboardError;

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
