//! Linux-only read-only FUSE mounts used by task-051, task-052 and task-058.
//!
//! The mount deliberately exposes only metadata until the first FUSE `read`.
//! The content factory can be backed by either a local probe source or the bounded
//! network reader used by task-052 without changing the URI/clipboard boundary.

use std::collections::{HashMap, HashSet};
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

/// One entry in a read-only virtual directory tree.
pub struct LinuxVirtualFileTreeEntry {
    relative_path: String,
    file: Option<LinuxVirtualFile>,
}

impl LinuxVirtualFileTreeEntry {
    pub fn file(relative_path: impl Into<String>, file: LinuxVirtualFile) -> io::Result<Self> {
        let relative_path = relative_path.into();
        validate_relative_path(&relative_path)?;
        if relative_path.rsplit('/').next() != Some(file.file_name()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "virtual tree file name does not match its relative path",
            ));
        }
        Ok(Self {
            relative_path,
            file: Some(file),
        })
    }

    pub fn directory(relative_path: impl Into<String>) -> io::Result<Self> {
        let relative_path = relative_path.into();
        validate_relative_path(&relative_path)?;
        Ok(Self {
            relative_path,
            file: None,
        })
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn is_directory(&self) -> bool {
        self.file.is_none()
    }
}

/// A validated set of files and directories exposed below one FUSE mount.
pub struct LinuxVirtualFileTree {
    entries: Vec<LinuxVirtualFileTreeEntry>,
}

impl LinuxVirtualFileTree {
    pub fn new(entries: Vec<LinuxVirtualFileTreeEntry>) -> io::Result<Self> {
        if entries.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "virtual tree must not be empty",
            ));
        }
        let mut paths = HashSet::with_capacity(entries.len());
        let mut kinds = HashMap::with_capacity(entries.len());
        for entry in &entries {
            if !paths.insert(entry.relative_path.clone()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "virtual tree contains duplicate paths",
                ));
            }
            kinds.insert(entry.relative_path.clone(), entry.is_directory());
        }
        for entry in &entries {
            let components: Vec<&str> = entry.relative_path.split('/').collect();
            for depth in 1..components.len() {
                let parent = components[..depth].join("/");
                if kinds.get(&parent) == Some(&false) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "virtual tree places an entry below a file",
                    ));
                }
            }
        }
        Ok(Self { entries })
    }

    fn into_entries(self) -> Vec<LinuxVirtualFileTreeEntry> {
        self.entries
    }
}

/// Handle for a mounted virtual directory tree.
pub struct LinuxVirtualFileTreeMount {
    session: Option<BackgroundSession>,
    root_paths: Vec<PathBuf>,
}

impl fmt::Debug for LinuxVirtualFileTreeMount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinuxVirtualFileTreeMount")
            .field("root_paths", &self.root_paths)
            .field("mounted", &self.session.is_some())
            .finish()
    }
}

impl LinuxVirtualFileTreeMount {
    pub fn mount(mount_point: impl AsRef<Path>, tree: LinuxVirtualFileTree) -> io::Result<Self> {
        let mount_point = mount_point.as_ref();
        validate_mount_point(mount_point)?;
        let filesystem = TreeFilesystem::new(tree)?;
        let root_paths = filesystem
            .root_names()
            .iter()
            .map(|name| mount_point.join(name))
            .collect();
        let mut config = Config::default();
        config.mount_options = vec![
            MountOption::RO,
            MountOption::NoDev,
            MountOption::NoSuid,
            MountOption::NoExec,
            MountOption::FSName("m590bridge-virtual-tree".into()),
        ];
        config.n_threads = Some(2);
        let session = fuser::spawn_mount(filesystem, mount_point, &config)?;
        Ok(Self {
            session: Some(session),
            root_paths,
        })
    }

    pub fn root_paths(&self) -> &[PathBuf] {
        &self.root_paths
    }

    pub fn unmount(mut self) -> io::Result<()> {
        self.session
            .take()
            .expect("virtual tree mount session already consumed")
            .umount_and_join()
    }
}

struct TreeNode {
    parent: INodeNo,
    name: String,
    kind: FileType,
    size: u64,
    children: Vec<INodeNo>,
    file: Option<LinuxVirtualFile>,
    reader: Mutex<ContentState>,
    content_handle: AtomicU64,
}

struct TreeFilesystem {
    nodes: HashMap<INodeNo, TreeNode>,
    paths: HashMap<String, INodeNo>,
    root_names: Vec<String>,
    next_inode: AtomicU64,
    next_handle: AtomicU64,
}

impl TreeFilesystem {
    fn new(tree: LinuxVirtualFileTree) -> io::Result<Self> {
        let mut filesystem = Self {
            nodes: HashMap::new(),
            paths: HashMap::new(),
            root_names: Vec::new(),
            next_inode: AtomicU64::new(2),
            next_handle: AtomicU64::new(1),
        };
        filesystem.nodes.insert(
            ROOT_INODE,
            TreeNode {
                parent: ROOT_INODE,
                name: String::new(),
                kind: FileType::Directory,
                size: 0,
                children: Vec::new(),
                file: None,
                reader: Mutex::new(ContentState::Unopened),
                content_handle: AtomicU64::new(NO_CONTENT_HANDLE),
            },
        );
        filesystem.paths.insert(String::new(), ROOT_INODE);

        for entry in tree.into_entries() {
            let components: Vec<&str> = entry.relative_path.split('/').collect();
            let parent_path = components[..components.len() - 1].join("/");
            let parent = filesystem.ensure_directory(&parent_path)?;
            let path = entry.relative_path.clone();
            let name = components
                .last()
                .expect("validated virtual tree path")
                .to_string();
            let kind = if entry.is_directory() {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            if let Some(existing) = filesystem.paths.get(&path).copied() {
                let node = filesystem
                    .nodes
                    .get(&existing)
                    .expect("virtual tree path index");
                if node.kind != kind {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "virtual tree entry changes file/directory kind",
                    ));
                }
                continue;
            }
            let inode = INodeNo(filesystem.next_inode.fetch_add(1, Ordering::Relaxed));
            let size = entry.file.as_ref().map_or(0, LinuxVirtualFile::size);
            filesystem.paths.insert(path, inode);
            filesystem.nodes.insert(
                inode,
                TreeNode {
                    parent,
                    name,
                    kind,
                    size,
                    children: Vec::new(),
                    file: entry.file,
                    reader: Mutex::new(ContentState::Unopened),
                    content_handle: AtomicU64::new(NO_CONTENT_HANDLE),
                },
            );
            filesystem
                .nodes
                .get_mut(&parent)
                .expect("virtual tree parent inode")
                .children
                .push(inode);
        }
        let names: HashMap<INodeNo, String> = filesystem
            .nodes
            .iter()
            .map(|(inode, node)| (*inode, node.name.clone()))
            .collect();
        for node in filesystem.nodes.values_mut() {
            node.children
                .sort_by_key(|child| names.get(child).cloned().unwrap_or_default());
        }
        filesystem.root_names = filesystem
            .nodes
            .get(&ROOT_INODE)
            .expect("virtual tree root")
            .children
            .iter()
            .filter_map(|inode| filesystem.nodes.get(inode).map(|node| node.name.clone()))
            .collect();
        Ok(filesystem)
    }

    fn root_names(&self) -> &[String] {
        &self.root_names
    }

    fn ensure_directory(&mut self, path: &str) -> io::Result<INodeNo> {
        if path.is_empty() {
            return Ok(ROOT_INODE);
        }
        let mut parent = ROOT_INODE;
        let mut current = String::new();
        for component in path.split('/') {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);
            if let Some(inode) = self.paths.get(&current).copied() {
                let node = self
                    .nodes
                    .get(&inode)
                    .expect("virtual tree directory index");
                if node.kind != FileType::Directory {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "virtual tree parent is a file",
                    ));
                }
                parent = inode;
                continue;
            }
            let inode = INodeNo(self.next_inode.fetch_add(1, Ordering::Relaxed));
            self.paths.insert(current.clone(), inode);
            self.nodes.insert(
                inode,
                TreeNode {
                    parent,
                    name: component.to_string(),
                    kind: FileType::Directory,
                    size: 0,
                    children: Vec::new(),
                    file: None,
                    reader: Mutex::new(ContentState::Unopened),
                    content_handle: AtomicU64::new(NO_CONTENT_HANDLE),
                },
            );
            self.nodes
                .get_mut(&parent)
                .expect("virtual tree parent inode")
                .children
                .push(inode);
            parent = inode;
        }
        Ok(parent)
    }

    fn attr_for(&self, inode: INodeNo, request: &Request) -> FileAttr {
        let node = self.nodes.get(&inode).expect("virtual tree inode");
        FileAttr {
            ino: inode,
            size: node.size,
            blocks: node.size.div_ceil(BLOCK_SIZE),
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: node.kind,
            perm: if node.kind == FileType::Directory {
                0o555
            } else {
                0o444
            },
            nlink: if node.kind == FileType::Directory {
                2
            } else {
                1
            },
            uid: request.uid(),
            gid: request.gid(),
            rdev: 0,
            blksize: BLOCK_SIZE as u32,
            flags: 0,
        }
    }

    fn read_range(
        &self,
        inode: INodeNo,
        handle: FileHandle,
        offset: u64,
        requested: u32,
    ) -> io::Result<Vec<u8>> {
        let node = self.nodes.get(&inode).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "virtual tree inode not found")
        })?;
        if node.kind != FileType::RegularFile {
            return Err(io::Error::from(io::ErrorKind::IsADirectory));
        }
        if offset >= node.size || requested == 0 {
            return Ok(Vec::new());
        }
        let remaining = node.size - offset;
        let length = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(usize::try_from(requested).unwrap_or(usize::MAX))
            .min(MAX_READ_SIZE);
        let mut state = node
            .reader
            .lock()
            .map_err(|_| io::Error::other("virtual tree reader lock poisoned"))?;
        if matches!(*state, ContentState::Unopened) {
            let file = node.file.as_ref().expect("virtual tree file node");
            match file.open_content() {
                Ok(reader) => {
                    node.content_handle.store(handle.0, Ordering::Release);
                    *state = ContentState::Opened(reader);
                }
                Err(error) => {
                    *state = ContentState::Failed {
                        kind: error.kind(),
                        message: error.to_string(),
                    };
                }
            }
        }
        let reader = match &mut *state {
            ContentState::Opened(reader) => reader,
            ContentState::Failed { kind, message } => {
                return Err(io::Error::new(*kind, message.clone()));
            }
            ContentState::Unopened => unreachable!("virtual tree content is opened or failed"),
        };
        reader.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0_u8; length];
        let mut filled = 0;
        while filled < data.len() {
            match reader.read(&mut data[filled..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "virtual tree content ended before its declared size",
                    ));
                }
                Ok(read) => filled += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(data)
    }

    fn open_empty_content(&self, inode: INodeNo, handle: FileHandle) -> io::Result<()> {
        let node = self.nodes.get(&inode).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "virtual tree inode not found")
        })?;
        if node.kind != FileType::RegularFile || node.size != 0 {
            return Ok(());
        }
        let mut state = node
            .reader
            .lock()
            .map_err(|_| io::Error::other("virtual tree reader lock poisoned"))?;
        if matches!(*state, ContentState::Unopened) {
            let file = node.file.as_ref().expect("virtual tree file node");
            match file.open_content() {
                Ok(reader) => {
                    node.content_handle.store(handle.0, Ordering::Release);
                    *state = ContentState::Opened(reader);
                }
                Err(error) => {
                    *state = ContentState::Failed {
                        kind: error.kind(),
                        message: error.to_string(),
                    };
                }
            }
        }
        match &mut *state {
            ContentState::Opened(reader) => {
                let mut probe = [0_u8; 1];
                match reader.read(&mut probe) {
                    Ok(0) => Ok(()),
                    Ok(_) => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "empty virtual file produced content",
                    )),
                    Err(error) => Err(error),
                }
            }
            ContentState::Failed { kind, message } => Err(io::Error::new(*kind, message.clone())),
            ContentState::Unopened => unreachable!("virtual tree content is opened or failed"),
        }
    }

    fn release_handle(&self, inode: INodeNo, handle: FileHandle) {
        let Some(node) = self.nodes.get(&inode) else {
            return;
        };
        if node
            .content_handle
            .compare_exchange(
                handle.0,
                NO_CONTENT_HANDLE,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if let Some(file) = node.file.as_ref() {
                file.release_content();
            }
            // Reset the per-inode content state so a second open (serial reopen of the
            // same clipboard offer) re-invokes the bridge factory instead of reusing
            // the now-exhausted reader.
            if let Ok(mut state) = node.reader.lock() {
                *state = ContentState::Unopened;
            }
        }
    }
}

impl Filesystem for TreeFilesystem {
    fn lookup(&self, request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let Some(parent_node) = self.nodes.get(&parent) else {
            reply.error(Errno::ENOENT);
            return;
        };
        let Some(name) = name.to_str() else {
            reply.error(Errno::ENOENT);
            return;
        };
        let Some(inode) = parent_node
            .children
            .iter()
            .copied()
            .find(|inode| self.nodes.get(inode).is_some_and(|node| node.name == name))
        else {
            reply.error(Errno::ENOENT);
            return;
        };
        reply.entry(
            &ATTRIBUTE_TTL,
            &self.attr_for(inode, request),
            fuser::Generation(0),
        );
    }

    fn getattr(
        &self,
        request: &Request,
        inode: INodeNo,
        _fh: Option<FileHandle>,
        reply: ReplyAttr,
    ) {
        if self.nodes.contains_key(&inode) {
            reply.attr(&ATTRIBUTE_TTL, &self.attr_for(inode, request));
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn open(&self, _request: &Request, inode: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        let Some(node) = self.nodes.get(&inode) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if node.kind != FileType::RegularFile {
            reply.error(Errno::EISDIR);
            return;
        }
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        if let Err(_error) = self.open_empty_content(inode, FileHandle(handle)) {
            self.release_handle(inode, FileHandle(handle));
            reply.error(Errno::EIO);
            return;
        }
        reply.opened(FileHandle(handle), FopenFlags::FOPEN_DIRECT_IO);
    }

    fn read(
        &self,
        _request: &Request,
        inode: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: ReplyData,
    ) {
        match self.read_range(inode, fh, offset, size) {
            Ok(data) => reply.data(&data),
            Err(_) => reply.error(Errno::EIO),
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
        if self
            .nodes
            .get(&inode)
            .is_some_and(|node| node.kind == FileType::RegularFile)
        {
            self.release_handle(inode, fh);
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
        let Some(node) = self.nodes.get(&inode) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if node.kind != FileType::Directory {
            reply.error(Errno::ENOTDIR);
            return;
        }
        let mut entries = vec![(inode, FileType::Directory, ".".to_string())];
        let parent = if inode == ROOT_INODE {
            ROOT_INODE
        } else {
            node.parent
        };
        entries.push((parent, FileType::Directory, "..".to_string()));
        entries.extend(node.children.iter().filter_map(|child| {
            self.nodes
                .get(child)
                .map(|item| (*child, item.kind, item.name.clone()))
        }));
        for (index, (entry_inode, kind, name)) in entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*entry_inode, (index + 1) as u64, *kind, name) {
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
        if self.nodes.contains_key(&inode) {
            reply.ok();
        } else {
            reply.error(Errno::ENOENT);
        }
    }
}

fn validate_relative_path(path: &str) -> io::Result<()> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "virtual tree path must be relative and use '/' separators",
        ));
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "virtual tree path contains an unsafe component",
            ));
        }
        validate_file_name(component)?;
    }
    Ok(())
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
            // Reset the content state so a second open (serial reopen of the same
            // clipboard offer) re-invokes the bridge factory instead of reusing the
            // now-exhausted reader.
            if let Ok(mut state) = self.reader.lock() {
                *state = ContentState::Unopened;
            }
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
    use std::ffi::OsString;
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

    #[test]
    fn tree_builds_explicit_implicit_and_empty_directories() {
        let nested =
            LinuxVirtualFile::new("nested.txt", 3, || Ok(Cursor::new(b"abc".to_vec()))).unwrap();
        let root =
            LinuxVirtualFile::new("root.txt", 4, || Ok(Cursor::new(b"root".to_vec()))).unwrap();
        let tree = LinuxVirtualFileTree::new(vec![
            LinuxVirtualFileTreeEntry::file("alpha/beta/nested.txt", nested).unwrap(),
            LinuxVirtualFileTreeEntry::directory("alpha").unwrap(),
            LinuxVirtualFileTreeEntry::directory("empty").unwrap(),
            LinuxVirtualFileTreeEntry::file("root.txt", root).unwrap(),
        ])
        .unwrap();
        let filesystem = TreeFilesystem::new(tree).unwrap();

        assert_eq!(filesystem.root_names(), ["alpha", "empty", "root.txt"]);
        for path in ["alpha", "alpha/beta", "empty"] {
            let inode = filesystem.paths[path];
            assert_eq!(filesystem.nodes[&inode].kind, FileType::Directory);
        }
        let nested_inode = filesystem.paths["alpha/beta/nested.txt"];
        assert_eq!(filesystem.nodes[&nested_inode].kind, FileType::RegularFile);
        assert_eq!(filesystem.nodes[&nested_inode].size, 3);
        assert!(filesystem.nodes[&filesystem.paths["empty"]]
            .children
            .is_empty());
    }

    #[test]
    fn tree_metadata_stays_lazy_and_files_open_independently() {
        let first_opened = Arc::new(AtomicUsize::new(0));
        let first_opened_for_factory = Arc::clone(&first_opened);
        let first = LinuxVirtualFile::new("one.bin", 3, move || {
            first_opened_for_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Cursor::new(b"one".to_vec()))
        })
        .unwrap();
        let second_opened = Arc::new(AtomicUsize::new(0));
        let second_opened_for_factory = Arc::clone(&second_opened);
        let second = LinuxVirtualFile::new("two.bin", 3, move || {
            second_opened_for_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Cursor::new(b"two".to_vec()))
        })
        .unwrap();
        let tree = LinuxVirtualFileTree::new(vec![
            LinuxVirtualFileTreeEntry::file("dir/one.bin", first).unwrap(),
            LinuxVirtualFileTreeEntry::file("two.bin", second).unwrap(),
        ])
        .unwrap();
        let filesystem = TreeFilesystem::new(tree).unwrap();

        assert_eq!(first_opened.load(Ordering::SeqCst), 0);
        assert_eq!(second_opened.load(Ordering::SeqCst), 0);
        let first_inode = filesystem.paths["dir/one.bin"];
        let second_inode = filesystem.paths["two.bin"];
        assert_eq!(filesystem.nodes[&first_inode].size, 3);
        assert_eq!(first_opened.load(Ordering::SeqCst), 0);
        assert_eq!(second_opened.load(Ordering::SeqCst), 0);

        assert_eq!(
            filesystem
                .read_range(first_inode, FileHandle(11), 0, 3)
                .unwrap(),
            b"one"
        );
        assert_eq!(first_opened.load(Ordering::SeqCst), 1);
        assert_eq!(second_opened.load(Ordering::SeqCst), 0);
        assert_eq!(
            filesystem
                .read_range(second_inode, FileHandle(22), 0, 3)
                .unwrap(),
            b"two"
        );
        assert_eq!(second_opened.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn tree_release_callback_belongs_to_the_read_file() {
        let first_released = Arc::new(AtomicUsize::new(0));
        let first_released_for_callback = Arc::clone(&first_released);
        let first = LinuxVirtualFile::new_with_release(
            "one.bin",
            1,
            || Ok(Cursor::new(b"1".to_vec())),
            move || {
                first_released_for_callback.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        let second_released = Arc::new(AtomicUsize::new(0));
        let second_released_for_callback = Arc::clone(&second_released);
        let second = LinuxVirtualFile::new_with_release(
            "two.bin",
            1,
            || Ok(Cursor::new(b"2".to_vec())),
            move || {
                second_released_for_callback.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        let tree = LinuxVirtualFileTree::new(vec![
            LinuxVirtualFileTreeEntry::file("one.bin", first).unwrap(),
            LinuxVirtualFileTreeEntry::file("two.bin", second).unwrap(),
        ])
        .unwrap();
        let filesystem = TreeFilesystem::new(tree).unwrap();
        let first_inode = filesystem.paths["one.bin"];
        let second_inode = filesystem.paths["two.bin"];

        filesystem
            .read_range(first_inode, FileHandle(10), 0, 1)
            .unwrap();
        filesystem
            .read_range(second_inode, FileHandle(20), 0, 1)
            .unwrap();
        filesystem.release_handle(first_inode, FileHandle(20));
        assert_eq!(first_released.load(Ordering::SeqCst), 0);
        assert_eq!(second_released.load(Ordering::SeqCst), 0);
        filesystem.release_handle(first_inode, FileHandle(10));
        assert_eq!(first_released.load(Ordering::SeqCst), 1);
        assert_eq!(second_released.load(Ordering::SeqCst), 0);
        filesystem.release_handle(second_inode, FileHandle(20));
        assert_eq!(second_released.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn opening_an_empty_tree_file_triggers_and_releases_its_content() {
        let opened = Arc::new(AtomicUsize::new(0));
        let opened_for_factory = Arc::clone(&opened);
        let released = Arc::new(AtomicUsize::new(0));
        let released_for_callback = Arc::clone(&released);
        let empty = LinuxVirtualFile::new_with_release(
            "empty.txt",
            0,
            move || {
                opened_for_factory.fetch_add(1, Ordering::SeqCst);
                Ok(Cursor::new(Vec::new()))
            },
            move || {
                released_for_callback.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        let tree =
            LinuxVirtualFileTree::new(vec![
                LinuxVirtualFileTreeEntry::file("empty.txt", empty).unwrap()
            ])
            .unwrap();
        let filesystem = TreeFilesystem::new(tree).unwrap();
        let inode = filesystem.paths["empty.txt"];

        assert_eq!(opened.load(Ordering::SeqCst), 0);
        filesystem
            .open_empty_content(inode, FileHandle(31))
            .unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        filesystem.release_handle(inode, FileHandle(31));
        assert_eq!(released.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[ignore = "requires a working /dev/fuse; run explicitly on a Linux desktop"]
    fn mounted_tree_smoke_browses_and_reads_nested_content() {
        let mount_point = std::env::temp_dir().join(format!(
            "m590bridge-tree-smoke-{}-{}",
            std::process::id(),
            UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        fs::create_dir(&mount_point).unwrap();
        let file =
            LinuxVirtualFile::new("note.txt", 5, || Ok(Cursor::new(b"hello".to_vec()))).unwrap();
        let tree = LinuxVirtualFileTree::new(vec![
            LinuxVirtualFileTreeEntry::file("folder/note.txt", file).unwrap(),
            LinuxVirtualFileTreeEntry::directory("folder/empty").unwrap(),
        ])
        .unwrap();
        let mount = LinuxVirtualFileTreeMount::mount(&mount_point, tree).unwrap();

        let root_names = fs::read_dir(&mount_point)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(root_names, vec![OsString::from("folder")]);
        let names = fs::read_dir(mount_point.join("folder"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![OsString::from("empty"), OsString::from("note.txt")]
        );
        assert_eq!(
            fs::read(mount_point.join("folder/note.txt")).unwrap(),
            b"hello"
        );

        mount.unmount().unwrap();
        fs::remove_dir(&mount_point).unwrap();
    }

    #[test]
    fn tree_rejects_unsafe_paths_duplicates_and_file_parents() {
        for path in ["", "/absolute", ".", "..", "a/../b", "a//b", "a\\b", "a\0b"] {
            assert!(
                LinuxVirtualFileTreeEntry::directory(path).is_err(),
                "{path:?}"
            );
        }

        let duplicate = LinuxVirtualFileTree::new(vec![
            LinuxVirtualFileTreeEntry::directory("same").unwrap(),
            LinuxVirtualFileTreeEntry::directory("same").unwrap(),
        ]);
        assert!(duplicate.is_err());

        let parent = LinuxVirtualFile::new("parent", 0, || Ok(Cursor::new(Vec::new()))).unwrap();
        let child = LinuxVirtualFile::new("child", 0, || Ok(Cursor::new(Vec::new()))).unwrap();
        let file_parent = LinuxVirtualFileTree::new(vec![
            LinuxVirtualFileTreeEntry::file("parent", parent).unwrap(),
            LinuxVirtualFileTreeEntry::file("parent/child", child).unwrap(),
        ]);
        assert!(file_parent.is_err());
    }
}
