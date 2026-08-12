use std::fmt;
use std::io::{Read, Seek};
use std::sync::Arc;

use crate::ClipboardError;

const WINDOWS_FILE_NAME_UTF16_CAPACITY: usize = 260;

pub(crate) trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

type OpenContent =
    dyn Fn() -> Result<Box<dyn ReadSeek + Send>, ClipboardError> + Send + Sync + 'static;

#[derive(Clone)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct VirtualFile {
    file_name: String,
    file_name_utf16: Vec<u16>,
    size: u64,
    open_content: Arc<OpenContent>,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl VirtualFile {
    pub fn new<F, R>(
        file_name: impl Into<String>,
        size: u64,
        open_content: F,
    ) -> Result<Self, ClipboardError>
    where
        F: Fn() -> Result<R, ClipboardError> + Send + Sync + 'static,
        R: Read + Seek + Send + 'static,
    {
        let file_name = file_name.into();
        let file_name_utf16 = validate_virtual_file_name(&file_name)?;
        Ok(Self {
            file_name,
            file_name_utf16,
            size,
            open_content: Arc::new(move || {
                open_content().map(|reader| Box::new(reader) as Box<dyn ReadSeek + Send>)
            }),
        })
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub(crate) fn file_name_utf16(&self) -> &[u16] {
        &self.file_name_utf16
    }

    pub(crate) fn open_content(&self) -> Result<Box<dyn ReadSeek + Send>, ClipboardError> {
        (self.open_content)()
    }
}

impl fmt::Debug for VirtualFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VirtualFile")
            .field("file_name", &self.file_name)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

fn validate_virtual_file_name(file_name: &str) -> Result<Vec<u16>, ClipboardError> {
    if file_name.is_empty() || file_name == "." || file_name == ".." {
        return Err(ClipboardError::Backend(
            "virtual file name must be a non-empty base name".into(),
        ));
    }
    if file_name.ends_with([' ', '.']) {
        return Err(ClipboardError::Backend(
            "virtual file name must not end with a space or dot".into(),
        ));
    }
    if file_name.chars().any(|ch| {
        ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        return Err(ClipboardError::Backend(
            "virtual file name contains Windows-invalid characters".into(),
        ));
    }

    let stem = file_name
        .split_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .trim_end_matches([' ', '.']);
    let upper_stem = stem.to_ascii_uppercase();
    let reserved = matches!(
        upper_stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        return Err(ClipboardError::Backend(
            "virtual file name uses a reserved Windows device name".into(),
        ));
    }

    let encoded: Vec<u16> = file_name.encode_utf16().collect();
    if encoded.len() >= WINDOWS_FILE_NAME_UTF16_CAPACITY {
        return Err(ClipboardError::Backend(format!(
            "virtual file name exceeds {} UTF-16 code units",
            WINDOWS_FILE_NAME_UTF16_CAPACITY - 1
        )));
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn validates_windows_safe_base_names() {
        let file =
            VirtualFile::new("报告 2026.txt", 3, || Ok(Cursor::new(vec![1_u8, 2, 3]))).unwrap();
        assert_eq!(file.file_name(), "报告 2026.txt");
        assert_eq!(file.size(), 3);
        assert_eq!(file.file_name_utf16().last(), Some(&('t' as u16)));
    }

    #[test]
    fn rejects_paths_reserved_names_and_oversized_names() {
        for name in [
            "", ".", "..", "a/b.txt", "a\\b.txt", "bad:name", "NUL.txt", "trail.",
        ] {
            assert!(
                VirtualFile::new(name, 0, || Ok(Cursor::new(Vec::<u8>::new()))).is_err(),
                "accepted {name:?}"
            );
        }
        let oversized = format!("{}.txt", "a".repeat(256));
        assert!(VirtualFile::new(oversized, 0, || Ok(Cursor::new(Vec::<u8>::new()))).is_err());
    }

    #[test]
    fn content_factory_is_lazy_and_repeatable() {
        let opened = Arc::new(AtomicUsize::new(0));
        let opened_for_factory = Arc::clone(&opened);
        let file = VirtualFile::new("lazy.bin", 4, move || {
            opened_for_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Cursor::new(vec![1_u8, 2, 3, 4]))
        })
        .unwrap();

        assert_eq!(opened.load(Ordering::SeqCst), 0);
        let _first = file.open_content().unwrap();
        let _second = file.open_content().unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 2);
    }
}
