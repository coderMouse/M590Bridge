use std::collections::{HashMap, HashSet};
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

/// One descriptor in a Windows virtual-file clipboard collection.
///
/// Paths use `/` internally and are converted to `\` only when building the
/// `FILEGROUPDESCRIPTORW` payload.
#[derive(Clone)]
pub struct VirtualFileCollectionEntry {
    relative_path: String,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    relative_path_utf16: Vec<u16>,
    file: Option<VirtualFile>,
}

impl VirtualFileCollectionEntry {
    pub fn file(
        relative_path: impl Into<String>,
        file: VirtualFile,
    ) -> Result<Self, ClipboardError> {
        let relative_path = relative_path.into();
        let relative_path_utf16 = validate_virtual_relative_path(&relative_path)?;
        let base_name = relative_path.rsplit('/').next().unwrap_or_default();
        if base_name != file.file_name() {
            return Err(ClipboardError::Backend(
                "virtual collection file name does not match its relative path".into(),
            ));
        }
        Ok(Self {
            relative_path,
            relative_path_utf16,
            file: Some(file),
        })
    }

    pub fn directory(relative_path: impl Into<String>) -> Result<Self, ClipboardError> {
        let relative_path = relative_path.into();
        let relative_path_utf16 = validate_virtual_relative_path(&relative_path)?;
        Ok(Self {
            relative_path,
            relative_path_utf16,
            file: None,
        })
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn is_directory(&self) -> bool {
        self.file.is_none()
    }

    pub fn size(&self) -> u64 {
        self.file.as_ref().map_or(0, VirtualFile::size)
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn relative_path_utf16(&self) -> &[u16] {
        &self.relative_path_utf16
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn file_contents(&self) -> Option<&VirtualFile> {
        self.file.as_ref()
    }
}

impl fmt::Debug for VirtualFileCollectionEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VirtualFileCollectionEntry")
            .field("relative_path", &self.relative_path)
            .field("directory", &self.is_directory())
            .field("size", &self.size())
            .finish()
    }
}

/// Ordered files and directories exposed through one OLE `IDataObject`.
#[derive(Clone, Debug)]
pub struct VirtualFileCollection {
    entries: Vec<VirtualFileCollectionEntry>,
    /// Optional plain DIB payload served as the Windows `CF_DIB` clipboard
    /// format. This is the bitmap format Word enumerates (task-061 traces show
    /// it never asks for `CF_DIBV5`), so it is what makes Word paste work.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    dib: Option<Vec<u8>>,
    /// Optional DIBv5 payload served as the Windows `CF_DIBV5` clipboard
    /// format when this collection also carries an image (Word/WordPad).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    dib_v5: Option<Vec<u8>>,
    /// Optional PNG payload served as the registered `PNG` clipboard format
    /// (used by `arboard` reads and image-aware apps).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    png: Option<Vec<u8>>,
}

impl VirtualFileCollection {
    pub fn new(entries: Vec<VirtualFileCollectionEntry>) -> Result<Self, ClipboardError> {
        if entries.is_empty() {
            return Err(ClipboardError::Backend(
                "virtual file collection must not be empty".into(),
            ));
        }
        if entries.len() > i32::MAX as usize {
            return Err(ClipboardError::Backend(
                "virtual file collection contains too many entries".into(),
            ));
        }

        let mut normalized_paths = HashSet::with_capacity(entries.len());
        let mut directory_by_path = HashMap::with_capacity(entries.len());
        for entry in &entries {
            let normalized = entry.relative_path.to_lowercase();
            if !normalized_paths.insert(normalized.clone()) {
                return Err(ClipboardError::Backend(
                    "virtual file collection contains case-insensitive duplicate paths".into(),
                ));
            }
            directory_by_path.insert(normalized, entry.is_directory());
        }
        for entry in &entries {
            let components: Vec<&str> = entry.relative_path.split('/').collect();
            for depth in 1..components.len() {
                let parent = components[..depth].join("/").to_lowercase();
                if directory_by_path.get(&parent) == Some(&false) {
                    return Err(ClipboardError::Backend(
                        "virtual file collection places an entry below a file".into(),
                    ));
                }
            }
        }
        Ok(Self {
            entries,
            dib: None,
            dib_v5: None,
            png: None,
        })
    }

    pub fn single(file: VirtualFile) -> Self {
        let entry = VirtualFileCollectionEntry {
            relative_path: file.file_name.clone(),
            relative_path_utf16: file.file_name_utf16.clone(),
            file: Some(file),
        };
        Self {
            entries: vec![entry],
            dib: None,
            dib_v5: None,
            png: None,
        }
    }

    /// A single virtual PNG file whose image is ALSO advertised as the Windows
    /// `CF_DIB` / `CF_DIBV5` and registered `PNG` clipboard formats. One OLE data
    /// object then serves Explorer (virtual file paste) and Word/WordPad (image
    /// paste).
    ///
    /// `CF_DIB` is the one that makes Word work: task-061 traces show Word never
    /// asks for `CF_DIBV5`, and with no format it recognized it never issued a
    /// `GetData` call at all.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub fn single_image(file: VirtualFile, dib: Vec<u8>, dib_v5: Vec<u8>, png: Vec<u8>) -> Self {
        Self {
            entries: vec![VirtualFileCollectionEntry {
                relative_path: file.file_name.clone(),
                relative_path_utf16: file.file_name_utf16.clone(),
                file: Some(file),
            }],
            dib: Some(dib),
            dib_v5: Some(dib_v5),
            png: Some(png),
        }
    }

    pub fn entries(&self) -> &[VirtualFileCollectionEntry] {
        &self.entries
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn dib_bytes(&self) -> Option<&[u8]> {
        self.dib.as_deref()
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn dib_v5_bytes(&self) -> Option<&[u8]> {
        self.dib_v5.as_deref()
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn png_bytes(&self) -> Option<&[u8]> {
        self.png.as_deref()
    }
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

    #[cfg(test)]
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

fn validate_virtual_relative_path(relative_path: &str) -> Result<Vec<u16>, ClipboardError> {
    if relative_path.is_empty()
        || relative_path.starts_with('/')
        || relative_path.contains('\0')
        || relative_path.contains('\\')
    {
        return Err(ClipboardError::Backend(
            "virtual collection path must be a safe slash-separated relative path".into(),
        ));
    }

    let mut encoded = Vec::new();
    for (index, component) in relative_path.split('/').enumerate() {
        let component_utf16 = validate_virtual_file_name(component)?;
        if index != 0 {
            encoded.push('\\' as u16);
        }
        encoded.extend(component_utf16);
    }
    if encoded.len() >= WINDOWS_FILE_NAME_UTF16_CAPACITY {
        return Err(ClipboardError::Backend(format!(
            "virtual collection path exceeds {} UTF-16 code units",
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

    #[test]
    fn collection_accepts_nested_files_and_directories_in_order() {
        let file = VirtualFile::new("report.txt", 3, || Ok(Cursor::new(b"abc".to_vec()))).unwrap();
        let collection = VirtualFileCollection::new(vec![
            VirtualFileCollectionEntry::directory("folder").unwrap(),
            VirtualFileCollectionEntry::directory("folder/empty").unwrap(),
            VirtualFileCollectionEntry::file("folder/report.txt", file).unwrap(),
        ])
        .unwrap();

        assert_eq!(collection.entries().len(), 3);
        assert!(collection.entries()[0].is_directory());
        assert_eq!(collection.entries()[2].size(), 3);
        assert_eq!(
            String::from_utf16(collection.entries()[2].relative_path_utf16()).unwrap(),
            "folder\\report.txt"
        );
    }

    #[test]
    fn collection_rejects_windows_collisions_and_file_parents() {
        let first = VirtualFile::new("a.txt", 0, || Ok(Cursor::new(Vec::<u8>::new()))).unwrap();
        let second = VirtualFile::new("A.TXT", 0, || Ok(Cursor::new(Vec::<u8>::new()))).unwrap();
        assert!(VirtualFileCollection::new(vec![
            VirtualFileCollectionEntry::file("folder/a.txt", first).unwrap(),
            VirtualFileCollectionEntry::file("FOLDER/A.TXT", second).unwrap(),
        ])
        .is_err());

        let parent = VirtualFile::new("folder", 0, || Ok(Cursor::new(Vec::<u8>::new()))).unwrap();
        let child = VirtualFile::new("child.txt", 0, || Ok(Cursor::new(Vec::<u8>::new()))).unwrap();
        assert!(VirtualFileCollection::new(vec![
            VirtualFileCollectionEntry::file("folder", parent).unwrap(),
            VirtualFileCollectionEntry::file("folder/child.txt", child).unwrap(),
        ])
        .is_err());
    }

    #[test]
    fn collection_rejects_unsafe_or_mismatched_relative_paths() {
        for path in ["", "/a.txt", "a\\b.txt", "a/../b.txt", "a/NUL.txt"] {
            assert!(
                VirtualFileCollectionEntry::directory(path).is_err(),
                "accepted {path:?}"
            );
        }
        let file = VirtualFile::new("a.txt", 0, || Ok(Cursor::new(Vec::<u8>::new()))).unwrap();
        assert!(VirtualFileCollectionEntry::file("folder/b.txt", file).is_err());
    }
}
