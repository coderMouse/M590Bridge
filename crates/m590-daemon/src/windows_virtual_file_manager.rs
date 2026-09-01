//! Windows STA owner for the OLE virtual-file clipboard object.

#![cfg(target_os = "windows")]

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use m590_clipboard::{
    publish_virtual_file_collection, pump_virtual_file_messages, VirtualFile, VirtualFileClipboard,
    VirtualFileCollection,
};

/// Upper bound for a confirmed publish. The STA loop wakes every 25ms and
/// `publish_virtual_file_collection` retries `CLIPBRD_E_CANT_OPEN` 10x25ms, so a
/// healthy publish answers well inside this; it only guards against a wedged STA.
const PUBLISH_ACK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerEvent {
    PublishFailed(String),
    ClipboardReplaced,
}

/// Whether the STA thread confirmed it finished `OleSetClipboard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// The OLE object is live and owns the clipboard.
    Confirmed,
    /// Queued, but the STA thread did not answer within [`PUBLISH_ACK_TIMEOUT`].
    /// The publish may still land, so callers must NOT fall back to a raw
    /// clipboard write: `EmptyClipboard` over an OLE owner corrupts ole32
    /// ownership and breaks every later `OleSetClipboard`.
    Unconfirmed,
}

enum Command {
    Publish {
        collection: VirtualFileCollection,
        /// `Some` makes the publish synchronous for the caller.
        ack: Option<Sender<()>>,
    },
    ReplaceIfCurrent {
        collection: VirtualFileCollection,
        result: Sender<bool>,
    },
    Clear,
    Stop,
}

pub struct WindowsVirtualFileManager {
    commands: Sender<Command>,
    events: Receiver<ManagerEvent>,
    thread: Option<JoinHandle<()>>,
}

impl WindowsVirtualFileManager {
    pub fn start() -> Result<Self, String> {
        let (commands, command_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("m590-ole-sta".into())
            .spawn(move || sta_loop(command_rx, event_tx))
            .map_err(|e| format!("spawn OLE STA: {e}"))?;
        Ok(Self {
            commands,
            events,
            thread: Some(thread),
        })
    }

    pub fn publish(&self, file: VirtualFile) -> Result<(), String> {
        self.publish_collection(VirtualFileCollection::single(file))
    }

    pub fn publish_collection(&self, collection: VirtualFileCollection) -> Result<(), String> {
        self.commands
            .send(Command::Publish {
                collection,
                ack: None,
            })
            .map_err(|_| "OLE STA stopped".into())
    }

    /// Publish and wait until the STA thread finished `OleSetClipboard`.
    ///
    /// The plain [`Self::publish_collection`] is fire-and-forget: it returns as
    /// soon as the command is queued, so the caller's thread can keep touching
    /// the clipboard (e.g. the local poll's `OpenClipboard`) while the STA thread
    /// is mid-publish. Callers that hold no other clipboard gate use this to
    /// close that overlap.
    pub fn publish_collection_synced(
        &self,
        collection: VirtualFileCollection,
    ) -> Result<PublishOutcome, String> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.commands
            .send(Command::Publish {
                collection,
                ack: Some(ack_tx),
            })
            .map_err(|_| "OLE STA stopped".to_string())?;
        match ack_rx.recv_timeout(PUBLISH_ACK_TIMEOUT) {
            Ok(()) => Ok(PublishOutcome::Confirmed),
            Err(_) => Ok(PublishOutcome::Unconfirmed),
        }
    }

    /// Replace an offer only while M590Bridge still owns the clipboard.
    ///
    /// This keeps a deferred remote offer from overwriting a native copy that
    /// replaced the original virtual-file clipboard during an active stream.
    pub fn replace_if_current(&self, file: VirtualFile) -> Result<bool, String> {
        self.replace_collection_if_current(VirtualFileCollection::single(file))
    }

    pub fn replace_collection_if_current(
        &self,
        collection: VirtualFileCollection,
    ) -> Result<bool, String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.commands
            .send(Command::ReplaceIfCurrent {
                collection,
                result: result_tx,
            })
            .map_err(|_| "OLE STA stopped".to_string())?;
        result_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "OLE STA did not answer conditional publish".into())
    }

    pub fn clear(&self) {
        let _ = self.commands.send(Command::Clear);
    }

    pub fn take_event(&self) -> Option<ManagerEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for WindowsVirtualFileManager {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn sta_loop(commands: Receiver<Command>, events: Sender<ManagerEvent>) {
    let mut guard: Option<VirtualFileClipboard> = None;
    loop {
        match commands.recv_timeout(Duration::from_millis(25)) {
            Ok(Command::Publish { collection, ack }) => {
                guard.take();
                match publish_virtual_file_collection(collection) {
                    Ok(next) => guard = Some(next),
                    Err(err) => {
                        let _ = events.send(ManagerEvent::PublishFailed(err.to_string()));
                    }
                }
                // Ack after the attempt either way: it only means "the STA thread
                // is done touching the clipboard". Failures keep reporting through
                // ManagerEvent::PublishFailed, so a waiting caller must not be
                // left blocking for PUBLISH_ACK_TIMEOUT on the error path.
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
            }
            Ok(Command::ReplaceIfCurrent { collection, result }) => {
                let owns_clipboard = guard.as_ref().is_some_and(VirtualFileClipboard::is_current);
                if !owns_clipboard {
                    guard.take();
                    let _ = result.send(false);
                    continue;
                }
                guard.take();
                match publish_virtual_file_collection(collection) {
                    Ok(next) => guard = Some(next),
                    Err(err) => {
                        let _ = events.send(ManagerEvent::PublishFailed(err.to_string()));
                    }
                }
                let _ = result.send(true);
            }
            Ok(Command::Clear) => {
                guard.take();
            }
            Ok(Command::Stop) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if let Some(current) = guard.as_ref() {
            if !current.is_current() {
                guard.take();
                let _ = events.send(ManagerEvent::ClipboardReplaced);
                continue;
            }
            let _ = current.pump_messages();
        } else {
            pump_virtual_file_messages();
        }
    }
    guard.take();
}
