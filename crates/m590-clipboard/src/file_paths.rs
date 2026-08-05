//! Helpers for turning clipboard file_list paths into offerable payloads.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ClipboardError;

/// First existing regular file in `paths` (skips dirs / missing).
pub fn first_regular_file(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.is_file()).cloned()
}

/// Read a local file for file-channel offer if it fits `max_bytes`.
///
/// Returns `(basename, data)`.
pub fn read_file_for_offer(
    path: &Path,
    max_bytes: usize,
) -> Result<(String, Vec<u8>), ClipboardError> {
    if !path.is_file() {
        return Err(ClipboardError::Backend(format!(
            "not a file: {}",
            path.display()
        )));
    }
    let meta = fs::metadata(path).map_err(|e| {
        ClipboardError::Backend(format!("stat {}: {e}", path.display()))
    })?;
    let len = meta.len() as usize;
    if len > max_bytes {
        return Err(ClipboardError::Backend(format!(
            "file too large for offer: {len}B > limit {max_bytes}B ({})",
            path.display()
        )));
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ClipboardError::Backend("file name missing".into()))?
        .to_string();
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(ClipboardError::Backend("invalid file name".into()));
    }
    let data = fs::read(path).map_err(|e| {
        ClipboardError::Backend(format!("read {}: {e}", path.display()))
    })?;
    Ok((name, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str, data: &[u8]) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("m590-fpath-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, data).unwrap();
        path
    }

    #[test]
    fn first_regular_skips_missing() {
        let p = temp_file("a.txt", b"hi");
        let missing = PathBuf::from("/no/such/m590-file-xyz");
        assert_eq!(
            first_regular_file(&[missing.clone(), p.clone()]).as_ref(),
            Some(&p)
        );
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn read_enforces_size() {
        let p = temp_file("b.bin", b"12345");
        let err = read_file_for_offer(&p, 4).unwrap_err().to_string();
        assert!(err.contains("too large"), "{err}");
        let (name, data) = read_file_for_offer(&p, 16).unwrap();
        assert_eq!(name, "b.bin");
        assert_eq!(data, b"12345");
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }
}
