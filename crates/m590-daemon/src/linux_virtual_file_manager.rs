//! Linux FUSE mount and clipboard ownership for one remote virtual file.

#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use m590_clipboard::ClipboardService;

use crate::linux_virtual_file::{LinuxVirtualFile, LinuxVirtualFileMount};

static NEXT_MOUNT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub struct LinuxVirtualFileManager {
    current: Option<MountedVirtualFile>,
}

impl LinuxVirtualFileManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(
        &mut self,
        clipboard: &mut dyn ClipboardService,
        file: LinuxVirtualFile,
    ) -> io::Result<()> {
        let next = MountedVirtualFile::mount(file)?;
        clipboard
            .write_file_list(&[next.file_path().to_path_buf()])
            .map_err(|error| io::Error::other(format!("publish FUSE file clipboard: {error}")))?;
        self.current = Some(next);
        Ok(())
    }

    /// Replace the current offer only while its exact FUSE path still owns the clipboard.
    pub fn replace_if_current(
        &mut self,
        clipboard: &mut dyn ClipboardService,
        file: LinuxVirtualFile,
    ) -> io::Result<bool> {
        if !self.is_current(clipboard)? {
            self.clear();
            return Ok(false);
        }
        self.publish(clipboard, file)?;
        Ok(true)
    }

    pub fn is_current(&self, clipboard: &mut dyn ClipboardService) -> io::Result<bool> {
        let Some(current) = self.current.as_ref() else {
            return Ok(false);
        };
        let paths = clipboard
            .read_file_list()
            .map_err(|error| io::Error::other(format!("read file clipboard: {error}")))?;
        Ok(paths.as_slice() == [current.file_path()])
    }

    pub fn clear(&mut self) {
        self.current = None;
    }
}

#[derive(Debug)]
struct MountedVirtualFile {
    mount: Option<LinuxVirtualFileMount>,
    mount_point: PathBuf,
}

impl MountedVirtualFile {
    fn mount(file: LinuxVirtualFile) -> io::Result<Self> {
        let mount_point = create_mount_point()?;
        match LinuxVirtualFileMount::mount(&mount_point, file) {
            Ok(mount) => Ok(Self {
                mount: Some(mount),
                mount_point,
            }),
            Err(error) => {
                let _ = fs::remove_dir(&mount_point);
                Err(error)
            }
        }
    }

    fn file_path(&self) -> &Path {
        self.mount
            .as_ref()
            .expect("mounted virtual file is present")
            .file_path()
    }
}

impl Drop for MountedVirtualFile {
    fn drop(&mut self) {
        if let Some(mount) = self.mount.take() {
            if let Err(error) = mount.unmount() {
                eprintln!("FUSE virtual file unmount failed: {error}");
            }
        }
        if let Err(error) = fs::remove_dir(&self.mount_point) {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("FUSE virtual file mount cleanup failed: {error}");
            }
        }
    }
}

fn create_mount_point() -> io::Result<PathBuf> {
    let base = std::env::temp_dir();
    for _ in 0..64 {
        let sequence = NEXT_MOUNT_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = base.join(mount_point_name(std::process::id(), sequence));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot allocate a unique FUSE mount directory",
    ))
}

fn mount_point_name(process_id: u32, sequence: u64) -> String {
    format!("m590bridge-fuse-{process_id}-{sequence}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use m590_clipboard::NullClipboard;

    #[test]
    fn clipboard_file_list_compares_exact_paths() {
        let expected = PathBuf::from("/tmp/m590bridge-fuse-test/file.bin");
        let mut clipboard = NullClipboard::new();
        clipboard
            .write_file_list(std::slice::from_ref(&expected))
            .unwrap();
        assert_eq!(clipboard.read_file_list().unwrap(), vec![expected]);

        clipboard
            .write_file_list(&[PathBuf::from("/tmp/another/file.bin")])
            .unwrap();
        assert_ne!(
            clipboard.read_file_list().unwrap(),
            vec![PathBuf::from("/tmp/m590bridge-fuse-test/file.bin")]
        );
    }

    #[test]
    fn mount_point_names_are_process_scoped_and_unique() {
        assert_eq!(mount_point_name(42, 1), "m590bridge-fuse-42-1");
        assert_ne!(mount_point_name(42, 1), mount_point_name(42, 2));
    }
}
