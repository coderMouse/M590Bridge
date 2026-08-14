//! Windows STA owner for the OLE virtual-file clipboard object.

#![cfg(target_os = "windows")]

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use m590_clipboard::{
    publish_virtual_file, pump_virtual_file_messages, VirtualFile, VirtualFileClipboard,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerEvent {
    PublishFailed(String),
    ClipboardReplaced,
}

enum Command {
    Publish(VirtualFile),
    ReplaceIfCurrent {
        file: VirtualFile,
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
        self.commands
            .send(Command::Publish(file))
            .map_err(|_| "OLE STA stopped".into())
    }

    /// Replace an offer only while M590Bridge still owns the clipboard.
    ///
    /// This keeps a deferred remote offer from overwriting a native copy that
    /// replaced the original virtual-file clipboard during an active stream.
    pub fn replace_if_current(&self, file: VirtualFile) -> Result<bool, String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.commands
            .send(Command::ReplaceIfCurrent {
                file,
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
            Ok(Command::Publish(file)) => {
                guard.take();
                match publish_virtual_file(file) {
                    Ok(next) => guard = Some(next),
                    Err(err) => {
                        let _ = events.send(ManagerEvent::PublishFailed(err.to_string()));
                    }
                }
            }
            Ok(Command::ReplaceIfCurrent { file, result }) => {
                let owns_clipboard = guard.as_ref().is_some_and(VirtualFileClipboard::is_current);
                if !owns_clipboard {
                    guard.take();
                    let _ = result.send(false);
                    continue;
                }
                guard.take();
                match publish_virtual_file(file) {
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
