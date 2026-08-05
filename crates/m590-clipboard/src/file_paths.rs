//! Helpers for turning clipboard file_list / path text into offerable payloads.

use std::fs;
use std::path::{Path, PathBuf};

use crate::image_file::{candidate_paths, is_likely_image_path, normalize_path_token};
use crate::ClipboardError;

fn scrub_path_string(s: &str) -> String {
    // text/uri-list often uses CRLF; arboard may leave trailing \r inside Path.
    s.trim()
        .trim_matches(|c| c == '\0' || c == '"')
        .trim_end_matches('\r')
        .trim()
        .to_string()
}

fn resolve_existing_file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    let raw = path.to_str()?;
    let cleaned = scrub_path_string(raw);
    if cleaned != raw {
        let p = PathBuf::from(&cleaned);
        if p.is_file() {
            return Some(p);
        }
    }
    // arboard usually returns real paths; still accept file://-looking Path display.
    normalize_path_token(&cleaned).filter(|p| p.is_file())
}

/// First existing regular file in `paths` (skips dirs / missing).
pub fn first_regular_file(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find_map(|p| resolve_existing_file(p))
}

/// If clipboard text is a local **non-image** file path/URI, return it.
pub fn regular_file_from_text(text: &str) -> Option<PathBuf> {
    for candidate in candidate_paths(text) {
        let Some(path) = resolve_existing_file(&candidate) else {
            continue;
        };
        if is_likely_image_path(&path) {
            continue;
        }
        return Some(path);
    }
    None
}

/// Read a local file for file-channel offer if it fits `max_bytes`.
///
/// Returns `(basename, data)`.
pub fn read_file_for_offer(
    path: &Path,
    max_bytes: usize,
) -> Result<(String, Vec<u8>), ClipboardError> {
    let path = resolve_existing_file(path).ok_or_else(|| {
        ClipboardError::Backend(format!("not a file: {}", path.display()))
    })?;
    let meta = fs::metadata(&path).map_err(|e| {
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
    let data = fs::read(&path).map_err(|e| {
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

    #[test]
    fn resolve_trims_trailing_cr() {
        let p = temp_file("cr.txt", b"x");
        let mut s = p.to_str().unwrap().to_string();
        s.push('\r');
        let dirty = PathBuf::from(&s);
        assert!(!dirty.is_file());
        assert_eq!(resolve_existing_file(&dirty).as_ref(), Some(&p));
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn regular_file_from_text_skips_images() {
        let p = temp_file("note.txt", b"hello");
        let t = p.to_str().unwrap();
        assert_eq!(regular_file_from_text(t).as_ref(), Some(&p));
        let img = temp_file("x.png", b"not-really-png");
        // extension says image → skip even if undecodable
        assert!(regular_file_from_text(img.to_str().unwrap()).is_none());
        let _ = fs::remove_dir_all(p.parent().unwrap());
        let _ = fs::remove_dir_all(img.parent().unwrap());
    }
}
