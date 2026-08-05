//! Safe on-disk helpers for the V2 file channel (hub inbox).

use std::fs;
use std::path::{Path, PathBuf};

/// Resolve a unique path under `dir` for basename `file_name`.
///
/// Rejects empty names and any name that is not a plain basename.
pub fn unique_save_path(dir: &Path, file_name: &str) -> Result<PathBuf, String> {
    let base = validate_basename(file_name)?;
    fs::create_dir_all(dir).map_err(|e| format!("create save dir: {e}"))?;

    let candidate = dir.join(&base);
    if !candidate.exists() {
        return Ok(candidate);
    }

    let (stem, ext) = split_name(&base);
    for i in 1..10_000 {
        let name = if ext.is_empty() {
            format!("{stem}-{i}")
        } else {
            format!("{stem}-{i}.{ext}")
        };
        let path = dir.join(name);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("too many name collisions in save dir".into())
}

/// Write `data` under `dir` using a safe unique basename.
pub fn save_received_file(dir: &Path, file_name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let path = unique_save_path(dir, file_name)?;
    fs::write(&path, data).map_err(|e| format!("write file: {e}"))?;
    Ok(path)
}

fn validate_basename(file_name: &str) -> Result<String, String> {
    let name = file_name.trim();
    if name.is_empty() {
        return Err("file name empty".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err("file name must be a basename".into());
    }
    if name == "." || name == ".." {
        return Err("file name invalid".into());
    }
    Ok(name.to_string())
}

fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !ext.contains('.') => {
            (stem.to_string(), ext.to_string())
        }
        _ => (name.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("m590-inbox-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_path_separators() {
        let dir = temp_dir();
        let err = save_received_file(&dir, "a/b.txt", b"x").unwrap_err();
        assert!(err.contains("basename"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_and_suffixes_on_collision() {
        let dir = temp_dir();
        let p1 = save_received_file(&dir, "note.txt", b"one").unwrap();
        let p2 = save_received_file(&dir, "note.txt", b"two").unwrap();
        assert_eq!(p1.file_name().unwrap(), "note.txt");
        assert_eq!(p2.file_name().unwrap(), "note-1.txt");
        assert_eq!(fs::read(&p1).unwrap(), b"one");
        assert_eq!(fs::read(&p2).unwrap(), b"two");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_transfer_then_save_like_hub() {
        use m590_core::{
            DeviceId, InboundFileResult, QueueFileResult, Session, SessionEvent,
        };

        let mut host = Session::new(DeviceId::new("host")).unwrap();
        host.handle(SessionEvent::StartPairing {
            expected_code: "999111".into(),
        })
        .unwrap();
        let _ = host.take_outbox();
        let mut joiner = Session::new(DeviceId::new("joiner")).unwrap();
        joiner
            .handle(SessionEvent::StartPairing {
                expected_code: "999111".into(),
            })
            .unwrap();
        // minimal pair exchange
        for msg in joiner.take_outbox() {
            host.handle(SessionEvent::Message(msg)).unwrap();
            for m in host.take_outbox() {
                joiner.handle(SessionEvent::Message(m)).unwrap();
            }
        }
        for msg in joiner.take_outbox() {
            host.handle(SessionEvent::Message(msg)).unwrap();
            for m in host.take_outbox() {
                joiner.handle(SessionEvent::Message(m)).unwrap();
            }
        }

        let data = b"hub-save-bytes".to_vec();
        assert_eq!(
            joiner
                .offer_file("t-save", "readme.txt", data.clone())
                .unwrap(),
            QueueFileResult::Queued
        );
        let offer = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(offer)).unwrap();
        assert!(matches!(
            host.take_inbound_file(),
            Some(InboundFileResult::Offered { .. })
        ));
        assert_eq!(host.request_file("t-save").unwrap(), QueueFileResult::Queued);
        let req = host.take_outbox().pop().unwrap();
        joiner.handle(SessionEvent::Message(req)).unwrap();
        for msg in joiner.take_outbox() {
            host.handle(SessionEvent::Message(msg)).unwrap();
        }
        let Some(InboundFileResult::Applied {
            file_name,
            data: got,
            ..
        }) = host.take_inbound_file()
        else {
            panic!("expected applied file");
        };
        assert_eq!(got, data);
        let dir = temp_dir();
        let saved = save_received_file(&dir, &file_name, &got).unwrap();
        assert_eq!(fs::read(saved).unwrap(), data);
        let _ = fs::remove_dir_all(&dir);
    }
}
