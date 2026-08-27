//! Linux FUSE mount and clipboard ownership for remote virtual files and trees.

#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use m590_clipboard::ClipboardService;

use crate::linux_virtual_file::{
    LinuxVirtualFile, LinuxVirtualFileMount, LinuxVirtualFileTree, LinuxVirtualFileTreeMount,
};

static NEXT_MOUNT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub struct LinuxVirtualFileManager {
    current: Option<MountedVirtualClipboard>,
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
        self.publish_mounted(
            clipboard,
            MountedVirtualClipboard::File(MountedVirtualFile::mount(file)?),
        )
    }

    pub fn publish_tree(
        &mut self,
        clipboard: &mut dyn ClipboardService,
        tree: LinuxVirtualFileTree,
    ) -> io::Result<()> {
        self.publish_mounted(
            clipboard,
            MountedVirtualClipboard::Tree(MountedVirtualTree::mount(tree)?),
        )
    }

    /// Replace the current offer only while its exact FUSE path still owns the clipboard.
    ///
    /// When the current offer is a single file, the existing mount-point
    /// directory is reused so the FUSE URI stays identical across a serial
    /// reopen (Nautilus "replace" stat keeps working). When the current
    /// offer is a tree (different path shape) the old mount is dropped and a
    /// fresh mount-point is allocated.
    pub fn replace_if_current(
        &mut self,
        clipboard: &mut dyn ClipboardService,
        file: LinuxVirtualFile,
    ) -> io::Result<bool> {
        if !self.is_current(clipboard)? {
            self.clear();
            return Ok(false);
        }
        let prev = self.current.take();
        let next = match prev {
            Some(MountedVirtualClipboard::File(mounted)) => {
                MountedVirtualClipboard::File(MountedVirtualFile::remount(file, mounted)?)
            }
            // Different shape: drop the old mount and allocate a fresh one.
            other => {
                drop(other);
                self.current = None;
                MountedVirtualClipboard::File(MountedVirtualFile::mount(file)?)
            }
        };
        clipboard
            .write_file_list(next.clipboard_paths())
            .map_err(|error| io::Error::other(format!("publish FUSE clipboard: {error}")))?;
        self.current = Some(next);
        Ok(true)
    }

    /// Replace the current offer with a tree only while its full path list owns the clipboard.
    ///
    /// When the current offer is a tree, the existing mount-point directory
    /// is reused so the root FUSE URIs stay identical across a serial
    /// reopen. When the current offer is a single file (different shape) the
    /// old mount is dropped and a fresh mount-point is allocated.
    pub fn replace_tree_if_current(
        &mut self,
        clipboard: &mut dyn ClipboardService,
        tree: LinuxVirtualFileTree,
    ) -> io::Result<bool> {
        if !self.is_current(clipboard)? {
            self.clear();
            return Ok(false);
        }
        let prev = self.current.take();
        let next = match prev {
            Some(MountedVirtualClipboard::Tree(mounted)) => {
                MountedVirtualClipboard::Tree(MountedVirtualTree::remount(tree, mounted)?)
            }
            // Different shape: drop the old mount and allocate a fresh one.
            other => {
                drop(other);
                self.current = None;
                MountedVirtualClipboard::Tree(MountedVirtualTree::mount(tree)?)
            }
        };
        clipboard
            .write_file_list(next.clipboard_paths())
            .map_err(|error| io::Error::other(format!("publish FUSE clipboard: {error}")))?;
        self.current = Some(next);
        Ok(true)
    }

    pub fn is_current(&self, clipboard: &mut dyn ClipboardService) -> io::Result<bool> {
        let Some(current) = self.current.as_ref() else {
            return Ok(false);
        };
        let paths = clipboard
            .read_file_list()
            .map_err(|error| io::Error::other(format!("read file clipboard: {error}")))?;
        Ok(paths.as_slice() == current.clipboard_paths())
    }

    pub fn clear(&mut self) {
        self.current = None;
    }

    fn publish_mounted(
        &mut self,
        clipboard: &mut dyn ClipboardService,
        next: MountedVirtualClipboard,
    ) -> io::Result<()> {
        clipboard
            .write_file_list(next.clipboard_paths())
            .map_err(|error| io::Error::other(format!("publish FUSE clipboard: {error}")))?;
        self.current = Some(next);
        Ok(())
    }
}

#[derive(Debug)]
enum MountedVirtualClipboard {
    File(MountedVirtualFile),
    Tree(MountedVirtualTree),
}

impl MountedVirtualClipboard {
    fn clipboard_paths(&self) -> &[PathBuf] {
        match self {
            Self::File(file) => std::slice::from_ref(&file.file_path),
            Self::Tree(tree) => tree.root_paths(),
        }
    }
}

#[derive(Debug)]
struct MountedVirtualFile {
    mount: Option<LinuxVirtualFileMount>,
    mount_point: PathBuf,
    file_path: PathBuf,
    reuse_mount_point: bool,
}

impl MountedVirtualFile {
    fn mount(file: LinuxVirtualFile) -> io::Result<Self> {
        let mount_point = create_mount_point()?;
        match LinuxVirtualFileMount::mount(&mount_point, file) {
            Ok(mount) => {
                let file_path = mount.file_path().to_path_buf();
                Ok(Self {
                    mount: Some(mount),
                    mount_point,
                    file_path,
                    reuse_mount_point: false,
                })
            }
            Err(error) => {
                let _ = fs::remove_dir(&mount_point);
                Err(error)
            }
        }
    }

    /// Reuse the existing mount-point directory so the FUSE URI stays
    /// identical across a serial reopen. The previous session is
    /// unmounted (the directory becomes empty) and a fresh one is mounted
    /// in the same path.
    fn remount(file: LinuxVirtualFile, prev: MountedVirtualFile) -> io::Result<Self> {
        let mount_point = prev.take_mount_point();
        // Reuse the same mount point directory; unmount already removed the
        // FUSE backing so the directory is empty again.
        match LinuxVirtualFileMount::mount(&mount_point, file) {
            Ok(mount) => {
                let file_path = mount.file_path().to_path_buf();
                Ok(Self {
                    mount: Some(mount),
                    mount_point,
                    file_path,
                    reuse_mount_point: false,
                })
            }
            Err(error) => {
                let _ = fs::remove_dir(&mount_point);
                Err(error)
            }
        }
    }
}

impl MountedVirtualFile {
    /// Unmount the live session but keep the mount-point directory so a
    /// subsequent [`Self::remount`] can reuse it (stable FUSE URI).
    fn take_mount_point(mut self) -> PathBuf {
        if let Some(mount) = self.mount.take() {
            if let Err(error) = mount.unmount() {
                eprintln!("FUSE virtual file unmount failed: {error}");
            }
        }
        let mount_point = self.mount_point.clone();
        // Prevent Drop from removing the directory we are handing off.
        self.reuse_mount_point = true;
        mount_point
    }
}

impl Drop for MountedVirtualFile {
    fn drop(&mut self) {
        if let Some(mount) = self.mount.take() {
            if let Err(error) = mount.unmount() {
                eprintln!("FUSE virtual file unmount failed: {error}");
            }
        }
        if !self.reuse_mount_point {
            if let Err(error) = fs::remove_dir(&self.mount_point) {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("FUSE virtual file mount cleanup failed: {error}");
                }
            }
        }
    }
}

#[derive(Debug)]
struct MountedVirtualTree {
    mount: Option<LinuxVirtualFileTreeMount>,
    mount_point: PathBuf,
    reuse_mount_point: bool,
}

impl MountedVirtualTree {
    fn mount(tree: LinuxVirtualFileTree) -> io::Result<Self> {
        let mount_point = create_mount_point()?;
        match LinuxVirtualFileTreeMount::mount(&mount_point, tree) {
            Ok(mount) => Ok(Self {
                mount: Some(mount),
                mount_point,
                reuse_mount_point: false,
            }),
            Err(error) => {
                let _ = fs::remove_dir(&mount_point);
                Err(error)
            }
        }
    }

    fn root_paths(&self) -> &[PathBuf] {
        self.mount
            .as_ref()
            .expect("mounted virtual tree is present")
            .root_paths()
    }

    /// Reuse the existing mount-point directory so all root FUSE URIs stay
    /// identical across a serial reopen. The previous session is unmounted
    /// (the directory becomes empty) and a fresh one is mounted in the same
    /// path.
    fn remount(tree: LinuxVirtualFileTree, prev: MountedVirtualTree) -> io::Result<Self> {
        let mount_point = prev.take_mount_point();
        match LinuxVirtualFileTreeMount::mount(&mount_point, tree) {
            Ok(mount) => Ok(Self {
                mount: Some(mount),
                mount_point,
                reuse_mount_point: false,
            }),
            Err(error) => {
                let _ = fs::remove_dir(&mount_point);
                Err(error)
            }
        }
    }
}

impl MountedVirtualTree {
    /// Unmount the live session but keep the mount-point directory so a
    /// subsequent [`Self::remount`] can reuse it (stable FUSE URIs).
    fn take_mount_point(mut self) -> PathBuf {
        if let Some(mount) = self.mount.take() {
            if let Err(error) = mount.unmount() {
                eprintln!("FUSE virtual tree unmount failed: {error}");
            }
        }
        let mount_point = self.mount_point.clone();
        // Prevent Drop from removing the directory we are handing off.
        self.reuse_mount_point = true;
        mount_point
    }
}

impl Drop for MountedVirtualTree {
    fn drop(&mut self) {
        if let Some(mount) = self.mount.take() {
            if let Err(error) = mount.unmount() {
                eprintln!("FUSE virtual tree unmount failed: {error}");
            }
        }
        if !self.reuse_mount_point {
            if let Err(error) = fs::remove_dir(&self.mount_point) {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("FUSE virtual tree mount cleanup failed: {error}");
                }
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
    fn clipboard_tree_list_preserves_all_top_level_paths() {
        let expected = vec![
            PathBuf::from("/tmp/m590bridge-fuse-test/dir"),
            PathBuf::from("/tmp/m590bridge-fuse-test/one.txt"),
            PathBuf::from("/tmp/m590bridge-fuse-test/two.txt"),
        ];
        let mut clipboard = NullClipboard::new();
        clipboard.write_file_list(&expected).unwrap();
        assert_eq!(clipboard.read_file_list().unwrap(), expected);
    }

    #[test]
    fn mount_point_names_are_process_scoped_and_unique() {
        assert_eq!(mount_point_name(42, 1), "m590bridge-fuse-42-1");
        assert_ne!(mount_point_name(42, 1), mount_point_name(42, 2));
    }
}
