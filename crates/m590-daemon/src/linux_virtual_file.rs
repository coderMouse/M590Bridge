//! Linux-only single-file FUSE mount used by task-051 and task-052.
//!
//! The mount deliberately exposes only metadata until the first FUSE `read`.
//! The content factory can be backed by either a local probe source or the bounded
//! network reader used by task-052 without changing the URI/clipboard boundary.

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use fuser::{
    BackgroundSession, Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags,
    INodeNo, MountOption, OpenFlags, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyOpen,
    Request,
};

const ROOT_INODE: INodeNo = INodeNo::ROOT;
const FILE_INODE: INodeNo = INodeNo(2);
const ATTRIBUTE_TTL: Duration = Duration::from_secs(0);
const BLOCK_SIZE: u64 = 512;
const MAX_READ_SIZE: usize = 1024 * 1024;
const NO_CONTENT_HANDLE: u64 = u64::MAX;

trait ReadSeek: Read + Seek + Send {}

impl<T: Read + Seek + Send> ReadSeek for T {}

type OpenContent = dyn Fn() -> io::Result<Box<dyn ReadSeek>> + Send + Sync + 'static;
type OnRelease = dyn Fn() + Send + Sync + 'static;

/// A single read-only file exposed by [`LinuxVirtualFileMount`].
#[derive(Clone)]
pub struct LinuxVirtualFile {
    file_name: String,
    size: u64,
    open_content: Arc<OpenContent>,
    on_release: Option<Arc<OnRelease>>,
}

impl LinuxVirtualFile {
    /// Build a virtual file with a lazy, repeatable content factory.
    pub fn new<F, R>(file_name: impl Into<String>, size: u64, open_content: F) -> io::Result<Self>
    where
        F: Fn() -> io::Result<R> + Send + Sync + 'static,
        R: Read + Seek + Send + 'static,
    {
        Self::new_inner(file_name, size, open_content, None::<fn()>)
    }

    pub(crate) fn new_with_release<F, R, C>(
        file_name: impl Into<String>,
        size: u64,
        open_content: F,
        on_release: C,
    ) -> io::Result<Self>
    where
        F: Fn() -> io::Result<R> + Send + Sync + 'static,
        R: Read + Seek + Send + 'static,
        C: Fn() + Send + Sync + 'static,
    {
        Self::new_inner(file_name, size, open_content, Some(on_release))
    }

    fn new_inner<F, R, C>(
        file_name: impl Into<String>,
        size: u64,
        open_content: F,
        on_release: Option<C>,
    ) -> io::Result<Self>
    where
        F: Fn() -> io::Result<R> + Send + Sync + 'static,
        R: Read + Seek + Send + 'static,
        C: Fn() + Send + Sync + 'static,
    {
        let file_name = file_name.into();
        validate_file_name(&file_name)?;
        Ok(Self {
            file_name,
            size,
            open_content: Arc::new(move || {
                open_content().map(|reader| Box::new(reader) as Box<dyn ReadSeek>)
            }),
            on_release: on_release.map(|callback| Arc::new(callback) as Arc<OnRelease>),
        })
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    fn open_content(&self) -> io::Result<Box<dyn ReadSeek>> {
        (self.open_content)()
    }

    fn release_content(&self) {
        if let Some(callback) = self.on_release.as_ref() {
            callback();
        }
    }
}

impl fmt::Debug for LinuxVirtualFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinuxVirtualFile")
            .field("file_name", &self.file_name)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Handle for a mounted single-file filesystem.
///
/// Dropping the handle unmounts the filesystem. [`Self::unmount`] can be used
/// when the caller needs to observe the unmount/join error.
pub struct LinuxVirtualFileMount {
    session: Option<BackgroundSession>,
    file_path: PathBuf,
}

impl fmt::Debug for LinuxVirtualFileMount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinuxVirtualFileMount")
            .field("file_path", &self.file_path)
            .field("mounted", &self.session.is_some())
            .finish()
    }
}

impl LinuxVirtualFileMount {
    /// Mount `file` below an existing empty directory and return its URI path.
    pub fn mount(mount_point: impl AsRef<Path>, file: LinuxVirtualFile) -> io::Result<Self> {
        let mount_point = mount_point.as_ref();
        validate_mount_point(mount_point)?;

        let mut config = Config::default();
        config.mount_options = vec![
            MountOption::RO,
            MountOption::NoDev,
            MountOption::NoSuid,
            MountOption::NoExec,
            MountOption::FSName("m590bridge-virtual-file".into()),
        ];
        // A second worker lets `release` cancel a network read blocked on the pipe.
        config.n_threads = Some(2);

        let file_path = mount_point.join(file.file_name());
        let filesystem = SingleFileFilesystem::new(file);
        let session = fuser::spawn_mount(filesystem, mount_point, &config)?;
        Ok(Self {
            session: Some(session),
            file_path,
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn unmount(mut self) -> io::Result<()> {
        self.session
            .take()
            .expect("virtual file mount session already consumed")
            .umount_and_join()
    }
}

struct SingleFileFilesystem {
    file: LinuxVirtualFile,
    reader: Mutex<ContentState>,
    next_handle: AtomicU64,
    content_handle: AtomicU64,
}

enum ContentState {
    Unopened,
    Opened(Box<dyn ReadSeek>),
    Failed {
        kind: io::ErrorKind,
        message: String,
    },
}

impl SingleFileFilesystem {
    fn new(file: LinuxVirtualFile) -> Self {
        Self {
            file,
            reader: Mutex::new(ContentState::Unopened),
            next_handle: AtomicU64::new(1),
            content_handle: AtomicU64::new(NO_CONTENT_HANDLE),
        }
    }

    fn root_attr(&self, request: &Request) -> FileAttr {
        self.root_attr_for_ids(request.uid(), request.gid())
    }

    fn root_attr_for_ids(&self, uid: u32, gid: u32) -> FileAttr {
        FileAttr {
            ino: ROOT_INODE,
            size: 0,
            blocks: 0,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::Directory,
            perm: 0o555,
            nlink: 2,
            uid,
            gid,
            rdev: 0,
            blksize: BLOCK_SIZE as u32,
            flags: 0,
        }
    }

    fn file_attr(&self, request: &Request) -> FileAttr {
        self.file_attr_for_ids(request.uid(), request.gid())
    }

    fn file_attr_for_ids(&self, uid: u32, gid: u32) -> FileAttr {
        FileAttr {
            ino: FILE_INODE,
            size: self.file.size,
            blocks: self.file.size.div_ceil(BLOCK_SIZE),
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o444,
            nlink: 1,
            uid,
            gid,
            rdev: 0,
            blksize: BLOCK_SIZE as u32,
            flags: 0,
        }
    }

    fn read_range(&self, handle: FileHandle, offset: u64, requested: u32) -> io::Result<Vec<u8>> {
        if offset >= self.file.size || requested == 0 {
            return Ok(Vec::new());
        }

        let remaining = self.file.size - offset;
        let length = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(usize::try_from(requested).unwrap_or(usize::MAX))
            .min(MAX_READ_SIZE);
        let mut state = self
            .reader
            .lock()
            .map_err(|_| io::Error::other("virtual file reader lock poisoned"))?;
        if matches!(*state, ContentState::Unopened) {
            match self.file.open_content() {
                Ok(reader) => {
                    self.content_handle.store(handle.0, Ordering::Release);
                    *state = ContentState::Opened(reader);
                }
                Err(error) => {
                    let failed = ContentState::Failed {
                        kind: error.kind(),
                        message: error.to_string(),
                    };
                    *state = failed;
                }
            }
        }

        let reader = match &mut *state {
            ContentState::Opened(reader) => reader,
            ContentState::Failed { kind, message } => {
                return Err(io::Error::new(*kind, message.clone()));
            }
            ContentState::Unopened => unreachable!("content state is opened or failed"),
        };
        reader.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0_u8; length];
        let mut filled = 0;
        while filled < data.len() {
            match reader.read(&mut data[filled..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "virtual file content ended before its declared size",
                    ));
                }
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(data)
    }

    fn release_handle(&self, handle: FileHandle) {
        if self
            .content_handle
            .compare_exchange(
                handle.0,
                NO_CONTENT_HANDLE,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            self.file.release_content();
        }
    }
}

impl Filesystem for SingleFileFilesystem {
    fn lookup(&self, request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        if parent == ROOT_INODE && name == OsStr::new(&self.file.file_name) {
            reply.entry(
                &ATTRIBUTE_TTL,
                &self.file_attr(request),
                fuser::Generation(0),
            );
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn getattr(
        &self,
        request: &Request,
        inode: INodeNo,
        _fh: Option<FileHandle>,
        reply: ReplyAttr,
    ) {
        match inode {
            ROOT_INODE => reply.attr(&ATTRIBUTE_TTL, &self.root_attr(request)),
            FILE_INODE => reply.attr(&ATTRIBUTE_TTL, &self.file_attr(request)),
            _ => reply.error(Errno::ENOENT),
        }
    }

    fn open(&self, _request: &Request, inode: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        if inode == FILE_INODE {
            let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
            reply.opened(FileHandle(handle), FopenFlags::FOPEN_DIRECT_IO);
        } else {
            reply.error(Errno::EISDIR);
        }
    }

    fn read(
        &self,
        _request: &Request,
        inode: INodeNo,
        _fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: ReplyData,
    ) {
        if inode != FILE_INODE {
            reply.error(Errno::ENOENT);
            return;
        }
        match self.read_range(_fh, offset, size) {
            Ok(data) => reply.data(&data),
            Err(_error) => reply.error(Errno::EIO),
        }
    }

    fn release(
        &self,
        _request: &Request,
        inode: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        if inode == FILE_INODE {
            self.release_handle(fh);
            reply.ok();
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn readdir(
        &self,
        _request: &Request,
        inode: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        if inode != ROOT_INODE {
            reply.error(Errno::ENOTDIR);
            return;
        }
        let entries = [
            (ROOT_INODE, FileType::Directory, "."),
            (ROOT_INODE, FileType::Directory, ".."),
            (
                FILE_INODE,
                FileType::RegularFile,
                self.file.file_name.as_str(),
            ),
        ];
        for (index, (entry_inode, kind, name)) in entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*entry_inode, (index + 1) as u64, *kind, *name) {
                break;
            }
        }
        reply.ok();
    }

    fn access(
        &self,
        _request: &Request,
        inode: INodeNo,
        _mask: fuser::AccessFlags,
        reply: fuser::ReplyEmpty,
    ) {
        if inode == ROOT_INODE || inode == FILE_INODE {
            reply.ok();
        } else {
            reply.error(Errno::ENOENT);
        }
    }
}

fn validate_file_name(file_name: &str) -> io::Result<()> {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.len() > 255
        || file_name
            .chars()
            .any(|character| character == '/' || character == '\0' || character.is_control())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "virtual file name must be one non-empty Linux base name (max 255 bytes)",
        ));
    }
    Ok(())
}

fn validate_mount_point(mount_point: &Path) -> io::Result<()> {
    if !mount_point.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "FUSE mount point must be absolute: {}",
                mount_point.display()
            ),
        ));
    }
    let metadata = fs::metadata(mount_point).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "cannot inspect FUSE mount point {}: {error}",
                mount_point.display()
            ),
        )
    })?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "FUSE mount point is not a directory: {}",
                mount_point.display()
            ),
        ));
    }
    let mut entries = fs::read_dir(mount_point)?;
    if entries.next().transpose()?.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("FUSE mount point must be empty: {}", mount_point.display()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn content_factory_stays_lazy_until_first_read() {
        let opened = Arc::new(AtomicUsize::new(0));
        let opened_for_factory = Arc::clone(&opened);
        let file = LinuxVirtualFile::new("lazy.bin", 6, move || {
            opened_for_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Cursor::new(b"abcdef".to_vec()))
        })
        .unwrap();
        let filesystem = SingleFileFilesystem::new(file);

        assert_eq!(opened.load(Ordering::SeqCst), 0);
        let _metadata = filesystem.file_attr_for_ids(1000, 1000);
        assert_eq!(opened.load(Ordering::SeqCst), 0);
        assert_eq!(filesystem.read_range(FileHandle(1), 2, 3).unwrap(), b"cde");
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        assert_eq!(filesystem.read_range(FileHandle(1), 0, 2).unwrap(), b"ab");
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn reads_declared_eof_and_rejects_short_source() {
        let file =
            LinuxVirtualFile::new("eof.bin", 3, || Ok(Cursor::new(b"abc".to_vec()))).unwrap();
        let filesystem = SingleFileFilesystem::new(file);
        assert_eq!(filesystem.read_range(FileHandle(1), 3, 10).unwrap(), b"");

        let short =
            LinuxVirtualFile::new("short.bin", 4, || Ok(Cursor::new(b"abc".to_vec()))).unwrap();
        let filesystem = SingleFileFilesystem::new(short);
        assert_eq!(
            filesystem
                .read_range(FileHandle(1), 0, 4)
                .unwrap_err()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn rejects_invalid_names_and_non_empty_mount_points() {
        for name in ["", ".", "..", "a/b", "a\0b"] {
            assert!(LinuxVirtualFile::new(name, 0, || Ok(Cursor::new(Vec::new()))).is_err());
        }

        let non_empty = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(validate_mount_point(non_empty).is_err());
        assert!(validate_mount_point(Path::new("relative-mount-point")).is_err());
    }

    #[test]
    fn only_the_content_handle_triggers_release_callback() {
        let released = Arc::new(AtomicUsize::new(0));
        let released_for_callback = Arc::clone(&released);
        let file = LinuxVirtualFile::new_with_release(
            "release.bin",
            3,
            || Ok(Cursor::new(b"abc".to_vec())),
            move || {
                released_for_callback.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        let filesystem = SingleFileFilesystem::new(file);
        assert_eq!(filesystem.read_range(FileHandle(7), 0, 1).unwrap(), b"a");
        filesystem.release_handle(FileHandle(8));
        assert_eq!(released.load(Ordering::SeqCst), 0);
        filesystem.release_handle(FileHandle(7));
        filesystem.release_handle(FileHandle(7));
        assert_eq!(released.load(Ordering::SeqCst), 1);
    }
}
