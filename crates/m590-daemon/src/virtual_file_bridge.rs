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
    finished: bool,
    cancelled: bool,
    error: Option<String>,
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
                finished: false,
                cancelled: false,
                error: None,
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
    if state.reader_open {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "virtual file content was opened more than once",
        ));
    }
    if state.cancelled {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "transfer cancelled",
        ));
    }
    state.reader_open = true;
    if !state.requested {
        state.requested = true;
        let _ = inner.events.send(BridgeEvent::Request);
    }
    inner.changed.notify_all();
    Ok(PipeReader {
        inner: Arc::clone(inner),
        position: 0,
        size,
    })
}

#[cfg(target_os = "linux")]
fn release_reader(inner: &Arc<PipeInner>, reason: &str) {
    if let Ok(mut state) = inner.state.lock() {
        if state.reader_open && !state.consumed && !state.cancelled {
            state.cancelled = true;
            let _ = inner.events.send(BridgeEvent::Cancel(reason.into()));
        }
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

    /// Push one verified network chunk, waiting for bounded capacity or cancellation.
    pub fn push(&self, data: &[u8]) -> io::Result<()> {
        self.push_with_timeout(data, WRITE_TIMEOUT)
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
                let should_cancel = true;
                if !should_cancel || state.cancelled {
                    self.inner.changed.notify_all();
                    return;
                }
                state.cancelled = true;
                let _ = self
                    .inner
                    .events
                    .send(BridgeEvent::Cancel("virtual file reader closed".into()));
            }
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
        release_reader(&bridge.inner, "virtual file reader closed");
        assert_eq!(
            bridge.take_event(),
            Some(BridgeEvent::Cancel("virtual file reader closed".into()))
        );
        assert_eq!(bridge.take_event(), Some(BridgeEvent::Released));
    }
}
