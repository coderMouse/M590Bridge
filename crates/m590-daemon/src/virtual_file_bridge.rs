//! Bounded byte pipes used to bridge network file chunks to OS virtual files.

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use m590_clipboard::{ClipboardError, VirtualFile};

#[cfg(target_os = "linux")]
use crate::linux_virtual_file::LinuxVirtualFile;

const DEFAULT_CAPACITY: usize = 4 * 1024 * 1024;
const WAIT_STEP: Duration = Duration::from_millis(100);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    Request,
    Consumed,
    Released,
    Cancel(String),
}

#[derive(Debug)]
struct PipeState {
    bytes: VecDeque<u8>,
    #[allow(dead_code)]
    requested: bool,
    started: bool,
    #[allow(dead_code)]
    reader_open: bool,
    consumed: bool,
    #[cfg(target_os = "linux")]
    released: bool,
    /// Linux: set by `open_reader` when a reader reopens while the previous
    /// network round is still winding down. While true, `try_push` refuses new
    /// bytes so stale leftovers of the old round can never reach the new reader.
    /// Cleared by `PipeProducer::arm()` once the hub has dispatched a fresh round.
    #[cfg(target_os = "linux")]
    resetting: bool,
    finished: bool,
    cancelled: bool,
    error: Option<String>,
    write_blocked_since: Option<Instant>,
}

#[derive(Debug)]
struct PipeInner {
    state: Mutex<PipeState>,
    changed: Condvar,
    capacity: usize,
    events: mpsc::Sender<BridgeEvent>,
}

/// Network-side producer for one virtual-file transfer.
#[derive(Debug, Clone)]
pub struct PipeProducer {
    inner: Arc<PipeInner>,
}

/// Reader exposed to the OLE `IStream` adapter.
#[derive(Debug)]
pub struct PipeReader {
    inner: Arc<PipeInner>,
    position: u64,
    size: u64,
}

/// Control handle retained by the hub/network loop.
#[derive(Debug)]
pub struct VirtualFileBridge {
    #[allow(dead_code)]
    inner: Arc<PipeInner>,
    events: mpsc::Receiver<BridgeEvent>,
}

impl VirtualFileBridge {
    pub fn new() -> (Self, PipeProducer) {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> (Self, PipeProducer) {
        assert!(capacity > 0, "virtual file pipe capacity must be positive");
        let (events_tx, events_rx) = mpsc::channel();
        let inner = Arc::new(PipeInner {
            state: Mutex::new(PipeState {
                bytes: VecDeque::new(),
                requested: false,
                started: false,
                reader_open: false,
                consumed: false,
                #[cfg(target_os = "linux")]
                released: false,
                #[cfg(target_os = "linux")]
                resetting: false,
                finished: false,
                cancelled: false,
                error: None,
                write_blocked_since: None,
            }),
            changed: Condvar::new(),
            capacity,
            events: events_tx,
        });
        (
            Self {
                inner: Arc::clone(&inner),
                events: events_rx,
            },
            PipeProducer { inner },
        )
    }

    pub fn take_event(&self) -> Option<BridgeEvent> {
        self.events.try_recv().ok()
    }

    #[cfg(target_os = "windows")]
    pub fn virtual_file(
        &self,
        file_name: impl Into<String>,
        size: u64,
    ) -> Result<VirtualFile, ClipboardError> {
        let bridge = Arc::clone(&self.inner);
        VirtualFile::new(file_name, size, move || {
            open_reader(&bridge, size).map_err(|err| ClipboardError::Backend(err.to_string()))
        })
    }

    #[cfg(target_os = "linux")]
    pub fn linux_virtual_file(
        &self,
        file_name: impl Into<String>,
        size: u64,
    ) -> io::Result<LinuxVirtualFile> {
        let reader_bridge = Arc::clone(&self.inner);
        let release_bridge = Arc::clone(&self.inner);
        LinuxVirtualFile::new_with_release(
            file_name,
            size,
            move || open_reader(&reader_bridge, size),
            move || release_reader(&release_bridge, "virtual file reader closed"),
        )
    }
}

#[allow(dead_code)]
fn open_reader(inner: &Arc<PipeInner>, size: u64) -> io::Result<PipeReader> {
    let mut state = inner.state.lock().map_err(poisoned)?;
    // Serial reopen: allow a second open only after the previous reader finished its
    // round cleanly (consumed/released). A still-open reader or a hard cancel blocks
    // reopening, mirroring the single active-stream invariant of protocol v3.
    if state.reader_open {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "virtual file content is already open",
        ));
    }
    if state.cancelled && !state.consumed && !state.finished {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "transfer cancelled",
        ));
    }
    // Reset the pipe for a fresh round so the new reader streams from offset 0.
    #[cfg(target_os = "linux")]
    let prior_round = state.consumed || state.finished || state.cancelled || state.released;
    #[cfg(not(target_os = "linux"))]
    let prior_round = state.consumed || state.finished || state.cancelled;
    if prior_round {
        state.bytes.clear();
        state.started = false;
        state.consumed = false;
        state.finished = false;
        state.cancelled = false;
        state.error = None;
        state.write_blocked_since = None;
        #[cfg(target_os = "linux")]
        {
            state.released = false;
            state.resetting = true;
        }
    }
    state.reader_open = true;
    state.requested = true;
    let _ = inner.events.send(BridgeEvent::Request);
    inner.changed.notify_all();
    Ok(PipeReader {
        inner: Arc::clone(inner),
        position: 0,
        size,
    })
}

#[cfg(target_os = "linux")]
fn release_reader(inner: &Arc<PipeInner>, _reason: &str) {
    if let Ok(mut state) = inner.state.lock() {
        // Do NOT set cancelled or send Cancel. A partial read (e.g. a Nautilus
        // thumbnail/metadata probe) closes the reader without consuming the full
        // file. Sending Cancel would tear down the network stream and, via the
        // hub's clipboard-poll/idle logic, unmount the FUSE before the user can
        // reopen the file for the actual paste/replace. Instead, just mark the
        // reader as released; open_reader will reset the pipe and re-request on
        // the next open. The producer's stalled-timeout is suppressed while
        // `released` is true so the sender does not abort prematurely.
        if state.reader_open && !state.released {
            state.released = true;
            let _ = inner.events.send(BridgeEvent::Released);
        }
        inner.changed.notify_all();
    }
}

fn mark_consumed(inner: &PipeInner, state: &mut PipeState) {
    if !state.consumed {
        state.consumed = true;
        let _ = inner.events.send(BridgeEvent::Consumed);
    }
}

impl PipeProducer {
    /// Start the timeout window once the corresponding network request is on the wire.
    pub fn start(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.started = true;
            self.inner.changed.notify_all();
        }
    }

    /// Linux: end the reopen window opened by `open_reader`. Call after the hub
    /// has dispatched a fresh network round so its chunks may fill the pipe.
    #[cfg(target_os = "linux")]
    pub fn arm(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.resetting = false;
            state.write_blocked_since = None;
            self.inner.changed.notify_all();
        }
    }

    /// Push one verified network chunk, waiting for bounded capacity or cancellation.
    pub fn push(&self, data: &[u8]) -> io::Result<()> {
        self.push_with_timeout(data, WRITE_TIMEOUT)
    }

    /// Try to enqueue one complete network chunk without waiting for the reader.
    ///
    /// Returning `Ok(false)` leaves the pipe unchanged. The hub can retain that
    /// one chunk and keep servicing cancellation/lifecycle events until capacity
    /// becomes available.
    pub fn try_push(&self, data: &[u8]) -> io::Result<bool> {
        self.try_push_with_timeout(data, WRITE_TIMEOUT)
    }

    fn try_push_with_timeout(&self, data: &[u8], timeout: Duration) -> io::Result<bool> {
        if data.is_empty() {
            return Ok(true);
        }
        if data.len() > self.inner.capacity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "virtual file chunk exceeds pipe capacity",
            ));
        }
        let mut state = self.inner.state.lock().map_err(poisoned)?;
        if state.cancelled {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "transfer cancelled",
            ));
        }
        // A reopened reader is waiting for a fresh network round. Refuse bytes
        // from the previous round (thumbnail-probe leftovers still in flight)
        // until the hub re-arms this bridge; otherwise the new reader would see
        // stale mid-file data before the sender restarts from offset 0.
        #[cfg(target_os = "linux")]
        if state.resetting {
            return Ok(false);
        }
        if self.inner.capacity - state.bytes.len() < data.len() {
            let blocked_since = *state.write_blocked_since.get_or_insert_with(Instant::now);
            // Suppress the stalled timeout while the reader has been released
            // (e.g. a Nautilus thumbnail probe that will reopen). The sender
            // should continue streaming; the pipe will be drained on reopen.
            #[cfg(target_os = "linux")]
            let suppress_timeout = state.released;
            #[cfg(not(target_os = "linux"))]
            let suppress_timeout = false;
            if blocked_since.elapsed() >= timeout && !suppress_timeout {
                state.cancelled = true;
                let message = "virtual file consumer stalled".to_string();
                let _ = self.inner.events.send(BridgeEvent::Cancel(message.clone()));
                self.inner.changed.notify_all();
                return Err(io::Error::new(io::ErrorKind::TimedOut, message));
            }
            return Ok(false);
        }
        state.write_blocked_since = None;
        state.started = true;
        state.bytes.extend(data);
        self.inner.changed.notify_all();
        Ok(true)
    }

    fn push_with_timeout(&self, data: &[u8], timeout: Duration) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let started = Instant::now();
        let mut offset = 0;
        while offset < data.len() {
            let mut state = self.inner.state.lock().map_err(poisoned)?;
            state.started = true;
            while state.bytes.len() >= self.inner.capacity && !state.cancelled {
                if started.elapsed() >= timeout {
                    state.cancelled = true;
                    let message = "virtual file consumer stalled".to_string();
                    let _ = self.inner.events.send(BridgeEvent::Cancel(message.clone()));
                    self.inner.changed.notify_all();
                    return Err(io::Error::new(io::ErrorKind::TimedOut, message));
                }
                let wait = timeout.saturating_sub(started.elapsed()).min(WAIT_STEP);
                state = self
                    .inner
                    .changed
                    .wait_timeout(state, wait)
                    .map_err(poisoned)?
                    .0;
            }
            if state.cancelled {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "transfer cancelled",
                ));
            }
            let room = self.inner.capacity.saturating_sub(state.bytes.len());
            let take = room.min(data.len() - offset);
            state.bytes.extend(&data[offset..offset + take]);
            offset += take;
            self.inner.changed.notify_all();
        }
        Ok(())
    }

    pub fn finish(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.started = true;
            state.finished = true;
            self.inner.changed.notify_all();
        }
    }

    pub fn fail(&self, message: impl Into<String>) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.started = true;
            state.error = Some(message.into());
            state.finished = true;
            self.inner.changed.notify_all();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner
            .state
            .lock()
            .map(|state| state.cancelled)
            .unwrap_or(true)
    }
}

impl Read for PipeReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let mut started = None;
        loop {
            let mut state = self.inner.state.lock().map_err(poisoned)?;
            if state.cancelled {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "transfer cancelled",
                ));
            }
            if !state.bytes.is_empty() {
                let take = output.len().min(state.bytes.len());
                for slot in &mut output[..take] {
                    *slot = state.bytes.pop_front().expect("checked non-empty");
                }
                self.position = self.position.saturating_add(take as u64);
                if self.position >= self.size {
                    mark_consumed(&self.inner, &mut state);
                }
                self.inner.changed.notify_all();
                return Ok(take);
            }
            if state.finished {
                if let Some(error) = state.error.clone() {
                    return Err(io::Error::other(error));
                }
                if self.position >= self.size {
                    mark_consumed(&self.inner, &mut state);
                }
                return Ok(0);
            }
            if state.started && started.is_none() {
                started = Some(Instant::now());
            }
            if started.is_some_and(|started| started.elapsed() >= READ_TIMEOUT) {
                state.cancelled = true;
                let _ = self
                    .inner
                    .events
                    .send(BridgeEvent::Cancel("virtual file read timeout".into()));
                self.inner.changed.notify_all();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "virtual file read timeout",
                ));
            }
            let wait = started.map_or(WAIT_STEP, |started| {
                READ_TIMEOUT
                    .saturating_sub(started.elapsed())
                    .min(WAIT_STEP)
            });
            state = self
                .inner
                .changed
                .wait_timeout(state, wait)
                .map_err(poisoned)?
                .0;
            drop(state);
        }
    }
}

impl Seek for PipeReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(value) => Some(value),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.size.checked_add_signed(delta),
        };
        match target {
            Some(value) if value == self.position => Ok(value),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "network-backed virtual file cannot seek away from current position",
            )),
        }
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.lock() {
            if self.position >= self.size {
                mark_consumed(&self.inner, &mut state);
            } else {
                #[cfg(target_os = "windows")]
                let should_cancel = !state.finished;
                #[cfg(not(target_os = "windows"))]
                let should_cancel = !state.released;
                if !should_cancel || state.cancelled {
                    state.reader_open = false;
                    self.inner.changed.notify_all();
                    return;
                }
                state.cancelled = true;
                let _ = self
                    .inner
                    .events
                    .send(BridgeEvent::Cancel("virtual file reader closed".into()));
            }
            state.reader_open = false;
            self.inner.changed.notify_all();
        }
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::other("virtual file pipe lock poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[cfg(target_os = "linux")]
    use std::fs;
    #[cfg(target_os = "linux")]
    use std::path::PathBuf;
    #[cfg(target_os = "linux")]
    use std::time::UNIX_EPOCH;

    #[cfg(target_os = "linux")]
    use crate::linux_virtual_file::{
        LinuxVirtualFileMount, LinuxVirtualFileTree, LinuxVirtualFileTreeEntry,
        LinuxVirtualFileTreeMount,
    };

    #[test]
    fn reader_requests_lazily_and_streams_with_backpressure() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(3);
        let mut reader = {
            #[cfg(target_os = "windows")]
            {
                let file = bridge.virtual_file("test.bin", 6).unwrap();
                file.open_content().unwrap()
            }
            #[cfg(not(target_os = "windows"))]
            {
                open_reader(&bridge.inner, 6).unwrap()
            }
        };
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        let producer_thread = thread::spawn(move || {
            producer.push(b"abcdef").unwrap();
            producer.finish();
        });
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        producer_thread.join().unwrap();
        assert_eq!(out, b"abcdef");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Consumed));
    }

    #[test]
    fn cancelled_reader_can_reopen_after_producer_fail() {
        // Simulate a Nautilus thumbnail probe: open, read a few bytes, drop
        // without consuming → Cancel. Then the hub soft-cancels via
        // producer.fail(). The next open must succeed (serial reopen).
        let (bridge, producer) = VirtualFileBridge::with_capacity(6);
        let producer2 = producer.clone();

        // First round: open and read partial data.
        let mut first = open_reader(&bridge.inner, 6).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        producer.push(b"abc").unwrap();
        let mut prefix = [0_u8; 3];
        first.read_exact(&mut prefix).unwrap();
        assert_eq!(&prefix, b"abc");

        // Drop the reader without consuming → Cancel.
        drop(first);
        assert_eq!(
            bridge.take_event(),
            Some(BridgeEvent::Cancel("virtual file reader closed".into()))
        );

        // Hub soft-cancel: producer.fail() sets finished=true.
        producer.fail("thumbnail probe");

        // Reopen must succeed despite cancelled=true (finished=true allows it).
        let mut second = open_reader(&bridge.inner, 6).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        let producer_thread = thread::spawn(move || {
            producer2.push(b"xyz123").unwrap();
            producer2.finish();
        });
        let mut out = Vec::new();
        second.read_to_end(&mut out).unwrap();
        producer_thread.join().unwrap();
        assert_eq!(out, b"xyz123");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Consumed));
    }

    #[test]
    fn completed_reader_can_be_reopened_for_a_second_stream() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(6);
        let producer2 = producer.clone();

        // First round.
        let mut first = open_reader(&bridge.inner, 6).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        let producer_thread = thread::spawn(move || {
            producer.push(b"abcdef").unwrap();
            producer.finish();
        });
        let mut out = Vec::new();
        first.read_to_end(&mut out).unwrap();
        producer_thread.join().unwrap();
        assert_eq!(out, b"abcdef");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Consumed));
        drop(first);

        // Second round: reopen after the first reader dropped.
        let mut second = open_reader(&bridge.inner, 6).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        let producer_thread2 = thread::spawn(move || {
            producer2.push(b"xyz123").unwrap();
            producer2.finish();
        });
        let mut out2 = Vec::new();
        second.read_to_end(&mut out2).unwrap();
        producer_thread2.join().unwrap();
        assert_eq!(out2, b"xyz123");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Consumed));
    }

    #[test]
    fn dropping_reader_cancels_producer() {
        let (bridge, producer) = VirtualFileBridge::new();
        let reader = open_reader(&bridge.inner, 1).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        drop(reader);
        assert_eq!(
            bridge.take_event(),
            Some(BridgeEvent::Cancel("virtual file reader closed".into()))
        );
        assert!(producer.push(b"x").is_err());
    }

    #[test]
    fn stalled_consumer_times_out_producer() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(1);
        let _reader = open_reader(&bridge.inner, 2).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        producer.push(b"a").unwrap();
        let err = producer
            .push_with_timeout(b"b", Duration::from_millis(10))
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(
            bridge.take_event(),
            Some(BridgeEvent::Cancel("virtual file consumer stalled".into()))
        );
    }

    #[test]
    fn try_push_applies_backpressure_without_partial_enqueue() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(3);
        let mut reader = open_reader(&bridge.inner, 5).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));

        assert!(producer.try_push(b"abc").unwrap());
        assert!(!producer.try_push(b"de").unwrap());
        let mut prefix = [0_u8; 2];
        reader.read_exact(&mut prefix).unwrap();
        assert_eq!(&prefix, b"ab");
        assert!(producer.try_push(b"de").unwrap());
        producer.finish();

        let mut suffix = Vec::new();
        reader.read_to_end(&mut suffix).unwrap();
        assert_eq!(suffix, b"cde");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Consumed));
    }

    #[test]
    fn try_push_rejects_a_chunk_larger_than_capacity() {
        let (_bridge, producer) = VirtualFileBridge::with_capacity(2);
        let error = producer.try_push(b"abc").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn cancelled_try_push_returns_without_waiting() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(1);
        let reader = open_reader(&bridge.inner, 2).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        assert!(producer.try_push(b"a").unwrap());
        drop(reader);

        let started = Instant::now();
        let error = producer.try_push(b"b").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn stalled_try_push_times_out_without_blocking_the_caller() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(1);
        let _reader = open_reader(&bridge.inner, 2).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        assert!(producer.try_push(b"a").unwrap());
        assert!(!producer
            .try_push_with_timeout(b"b", Duration::from_millis(5))
            .unwrap());
        thread::sleep(Duration::from_millis(10));

        let started = Instant::now();
        let error = producer
            .try_push_with_timeout(b"b", Duration::from_millis(5))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(
            bridge.take_event(),
            Some(BridgeEvent::Cancel("virtual file consumer stalled".into()))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a working /dev/fuse; run explicitly on a Linux desktop"]
    fn mounted_single_and_tree_stream_large_files_with_nonblocking_backpressure() {
        const FILE_SIZE: usize = 24 * 1024 * 1024 + 123;

        let single_mount_point = fuse_smoke_mount_point("large-single");
        fs::create_dir(&single_mount_point).unwrap();
        let (single_bridge, single_producer) = VirtualFileBridge::new();
        let single_file = single_bridge
            .linux_virtual_file("large-single.bin", FILE_SIZE as u64)
            .unwrap();
        let single_mount = LinuxVirtualFileMount::mount(&single_mount_point, single_file).unwrap();
        stream_and_verify_pattern(
            &single_bridge,
            single_producer,
            single_mount.file_path().to_path_buf(),
            FILE_SIZE,
        );
        single_mount.unmount().unwrap();
        fs::remove_dir(&single_mount_point).unwrap();

        let tree_mount_point = fuse_smoke_mount_point("large-tree");
        fs::create_dir(&tree_mount_point).unwrap();
        let (tree_bridge, tree_producer) = VirtualFileBridge::new();
        let tree_file = tree_bridge
            .linux_virtual_file("large-tree.bin", FILE_SIZE as u64)
            .unwrap();
        let tree = LinuxVirtualFileTree::new(vec![LinuxVirtualFileTreeEntry::file(
            "folder/large-tree.bin",
            tree_file,
        )
        .unwrap()])
        .unwrap();
        let tree_mount = LinuxVirtualFileTreeMount::mount(&tree_mount_point, tree).unwrap();
        stream_and_verify_pattern(
            &tree_bridge,
            tree_producer,
            tree_mount_point.join("folder/large-tree.bin"),
            FILE_SIZE,
        );
        tree_mount.unmount().unwrap();
        fs::remove_dir(&tree_mount_point).unwrap();
    }

    #[cfg(target_os = "linux")]
    fn fuse_smoke_mount_point(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "m590bridge-{label}-{}-{}",
            std::process::id(),
            UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ))
    }

    #[cfg(target_os = "linux")]
    fn stream_and_verify_pattern(
        bridge: &VirtualFileBridge,
        producer: PipeProducer,
        file_path: PathBuf,
        file_size: usize,
    ) {
        const NETWORK_CHUNK_SIZE: usize = 256 * 1024;

        let reader = thread::spawn(move || fs::read(file_path).unwrap());
        let request_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if bridge.take_event() == Some(BridgeEvent::Request) {
                break;
            }
            assert!(
                Instant::now() < request_deadline,
                "mounted file did not request its content"
            );
            thread::sleep(Duration::from_millis(1));
        }

        producer.start();
        let mut offset = 0;
        while offset < file_size {
            let length = NETWORK_CHUNK_SIZE.min(file_size - offset);
            let chunk = (offset..offset + length)
                .map(|index| (index % 251) as u8)
                .collect::<Vec<_>>();
            loop {
                if producer.try_push(&chunk).unwrap() {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
            }
            offset += length;
        }
        producer.finish();

        let received = reader.join().unwrap();
        assert_eq!(received.len(), file_size);
        assert!(received
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == (index % 251) as u8));
    }

    #[test]
    fn completed_pipe_still_cancels_when_reader_closes_early() {
        let (bridge, producer) = VirtualFileBridge::new();
        let mut reader = open_reader(&bridge.inner, 2).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        producer.push(b"ab").unwrap();
        producer.finish();
        let mut first = [0_u8; 1];
        assert_eq!(reader.read(&mut first).unwrap(), 1);
        drop(reader);
        assert_eq!(
            bridge.take_event(),
            Some(BridgeEvent::Cancel("virtual file reader closed".into()))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn releasing_linux_content_notifies_the_hub() {
        let (bridge, _producer) = VirtualFileBridge::new();
        let _reader = open_reader(&bridge.inner, 1).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        // release_reader no longer sends Cancel — it only marks the reader
        // as released so the pipe can be reset on reopen.
        release_reader(&bridge.inner, "virtual file reader closed");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Released));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reopened_reader_ignores_old_round_pushes_until_armed() {
        // Round 1: a Nautilus thumbnail probe opens the FUSE file, reads a few
        // bytes, and releases while the network stream is still in flight.
        let (bridge, producer) = VirtualFileBridge::with_capacity(16);
        let producer2 = producer.clone();
        let mut first = open_reader(&bridge.inner, 8).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));
        assert!(producer.try_push(b"01234567").unwrap());
        let mut prefix = [0_u8; 3];
        first.read_exact(&mut prefix).unwrap();
        assert_eq!(&prefix, b"012");
        release_reader(&bridge.inner, "virtual file reader closed");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Released));
        drop(first);

        // Round 2: the paste reopens the FUSE file. open_reader resets the pipe
        // and marks the bridge as `resetting` until the hub re-arms it.
        let mut second = open_reader(&bridge.inner, 8).unwrap();
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Request));

        // Leftover bytes of the old round must be refused: the new reader must
        // never see mid-file data of the previously cancelled round.
        assert!(!producer2.try_push(b"4567").unwrap());

        // The hub re-arms after dispatching a fresh FileRequest; a fresh round
        // then streams from offset 0 and reaches the reader untouched.
        producer2.arm();
        assert!(producer2.try_push(b"abcdefgh").unwrap());
        producer2.finish();
        let mut out = Vec::new();
        second.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"abcdefgh");
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Consumed));
    }
}
