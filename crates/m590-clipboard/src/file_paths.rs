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

/// Directories where bare clipboard filenames (GNOME desktop icon copy) may resolve.
pub fn file_search_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push_dir = |p: PathBuf| {
        if p.is_dir() && !out.iter().any(|e| e == &p) {
            out.push(p);
        }
    };

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        push_dir(home.join("桌面"));
        push_dir(home.join("Desktop"));
        // ~/.config/user-dirs.dirs XDG_DESKTOP_DIR=...
        let user_dirs = home.join(".config/user-dirs.dirs");
        if let Ok(contents) = fs::read_to_string(&user_dirs) {
            for line in contents.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("XDG_DESKTOP_DIR=") {
                    let val = rest.trim().trim_matches('"');
                    let val = val.replace("$HOME", &home.to_string_lossy());
                    if !val.is_empty() {
                        push_dir(PathBuf::from(val));
                    }
                }
            }
        }
        push_dir(home.join("Downloads"));
        push_dir(home.join("下载"));
        push_dir(home);
    }
    out
}

fn is_bare_filename(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return false;
    }
    // Reject multi-word sentences without extension-ish token.
    if name.contains(' ') && !name.contains('.') {
        return false;
    }
    true
}

fn resolve_existing_path(path: &Path) -> Option<PathBuf> {
    if path.is_file() || path.is_dir() {
        return Some(path.to_path_buf());
    }
    let raw = path.to_str()?;
    let cleaned = scrub_path_string(raw);
    if cleaned != raw {
        let p = PathBuf::from(&cleaned);
        if p.is_file() || p.is_dir() {
            return Some(p);
        }
    }
    if let Some(p) = normalize_path_token(&cleaned) {
        if p.is_file() || p.is_dir() {
            return Some(p);
        }
        // Bare / relative single component → search desktop dirs.
        if p.components().count() == 1 || is_bare_filename(&cleaned) {
            let name = p.file_name()?.to_os_string();
            for dir in file_search_dirs() {
                let candidate = dir.join(&name);
                if candidate.is_file() || candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    } else if is_bare_filename(&cleaned) {
        for dir in file_search_dirs() {
            let candidate = dir.join(&cleaned);
            if candidate.is_file() || candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

fn resolve_existing_file(path: &Path) -> Option<PathBuf> {
    resolve_existing_path(path).filter(|path| path.is_file())
}

/// First existing regular file in `paths` (skips dirs / missing).
pub fn first_regular_file(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find_map(|p| resolve_existing_file(p))
}

/// Return every existing local file or directory represented by clipboard text.
///
/// GNOME/Nautilus may expose a copied selection as a multi-line text payload
/// (`text/uri-list` or `x-special/gnome-copied-files`) when the platform file-list
/// API is unavailable. Keep all unique existing paths so callers can preserve
/// multi-file and directory semantics instead of silently taking the first file.
pub fn local_paths_from_text(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for candidate in candidate_paths(text) {
        let Some(path) = resolve_existing_path(&candidate) else {
            continue;
        };
        if !out.iter().any(|existing| existing == &path) {
            out.push(path);
        }
    }

    // GNOME desktop-icon copies can contain bare names rather than file:// URIs.
    // Resolve those against the same desktop/search directories used by the
    // single-file fallback, while retaining directory selections as well.
    for line in text.lines() {
        let line = scrub_path_string(line);
        let lower = line.to_ascii_lowercase();
        if lower == "copy" || lower == "cut" || lower.starts_with("x-special/") {
            continue;
        }
        if !is_bare_filename(&line) {
            continue;
        }
        for dir in file_search_dirs() {
            let path = dir.join(&line);
            if resolve_existing_path(&path).is_some()
                && !out.iter().any(|existing| existing == &path)
            {
                out.push(path);
                break;
            }
        }
    }
    out
}

/// Parse URI-list/GNOME copied-files text without requiring the paths to exist.
/// Used by the Linux Wayland MIME fallback; filesystem validation happens later
/// during batch scanning or single-file offer creation.
#[cfg(target_os = "linux")]
pub(crate) fn paths_from_file_list_text(text: &str) -> Vec<PathBuf> {
    candidate_paths(text)
}

/// If clipboard text is a local **non-image** file path/URI (or bare desktop name), return it.
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
    // GNOME desktop icon copy often yields only the basename as text.
    let trimmed = scrub_path_string(text);
    // Single-line bare name, or last non-noise line of gnome-copied-files style.
    let mut names: Vec<&str> = Vec::new();
    if is_bare_filename(&trimmed) && !trimmed.contains('\n') {
        names.push(trimmed.as_str());
    }
    for line in text.lines() {
        let line = line.trim();
        let lower = line.to_ascii_lowercase();
        if lower == "copy" || lower == "cut" || lower.starts_with("x-special/") {
            continue;
        }
        if is_bare_filename(line) {
            names.push(line);
        }
    }
    for name in names {
        for dir in file_search_dirs() {
            let path = dir.join(name);
            if path.is_file() && !is_likely_image_path(&path) {
                return Some(path);
            }
        }
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
    let path = resolve_existing_file(path)
        .ok_or_else(|| ClipboardError::Backend(format!("not a file: {}", path.display())))?;
    let meta = fs::metadata(&path)
        .map_err(|e| ClipboardError::Backend(format!("stat {}: {e}", path.display())))?;
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
    let data = fs::read(&path)
        .map_err(|e| ClipboardError::Backend(format!("read {}: {e}", path.display())))?;
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

    #[test]
    fn local_paths_from_text_keeps_multiline_files_and_directories() {
        let root = temp_file("placeholder.txt", b"placeholder")
            .parent()
            .unwrap()
            .to_path_buf();
        let first = root.join("first.txt");
        let second = root.join("second.csv");
        let nested = root.join("nested");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        fs::create_dir(&nested).unwrap();

        let text = format!(
            "copy\nfile://{}\nfile://{}\nfile://{}\n",
            first.display(),
            second.display(),
            nested.display()
        );
        let paths = local_paths_from_text(&text);
        assert_eq!(paths, vec![first, second, nested]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bare_filename_resolves_via_search_dir() {
        let p = temp_file("desk-note.txt", b"hi");
        let dir = p.parent().unwrap().to_path_buf();
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(dir.join(name).is_file());
        assert!(is_bare_filename(name));
        assert!(!is_bare_filename("hello world"));
        assert!(!is_bare_filename("/tmp/a.txt"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn bare_desktop_name_resolves_under_home_desktop() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("m590-home-{nanos}"));
        let desk = home.join("桌面");
        fs::create_dir_all(&desk).unwrap();
        let file = desk.join("12.txt");
        fs::write(&file, b"from-desktop").unwrap();
        // SAFETY: test-only HOME override for path resolution helpers.
        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", &home);
        let got = regular_file_from_text("12.txt");
        match prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        assert_eq!(got.as_ref(), Some(&file), "got={got:?}");
        let _ = fs::remove_dir_all(home);
    }
}
