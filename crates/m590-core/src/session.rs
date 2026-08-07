use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{
    bytes_to_hex, ConnectionState, DeviceId, FileChunkPayload, FileCompletePayload,
    FileOfferPayload, FileRequestPayload, Message, ProtocolError, SessionError, SyncState,
    PROTOCOL_VERSION,
};

/// Max remembered clipboard content IDs for dedup (send + receive).
const SEEN_CONTENT_ID_CAP: usize = 64;

/// Outbound file chunk size (streamed disk/network buffer).
pub const FILE_CHUNK_SIZE: usize = 256 * 1024;

/// How many chunks to emit per [`Session::pump_outbound_file`] call (back-pressure).
pub const OUTBOUND_CHUNKS_PER_PUMP: usize = 4;

/// Soft cap for any single file transfer (path or memory). Not an in-memory ceiling.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Max bytes accepted by the in-memory [`Session::offer_file`] / base64 path.
pub const MAX_MEMORY_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Default missed heartbeat-ack ticks before the peer is considered suspect.
pub const DEFAULT_HEARTBEAT_MISS_THRESHOLD: u32 = 3;

/// Events that drive the 1-on-1 session draft state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// Local side starts pairing with an expected code (host or joiner).
    StartPairing { expected_code: String },
    /// A decoded protocol message arrived (from memory mock or future TCP).
    Message(Message),
    /// Local liveness tick; only meaningful while connected.
    HeartbeatTick,
    /// Local or transport-level disconnect.
    Disconnect,
}

/// Result of trying to queue an outbound clipboard text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueClipboardResult {
    /// Frame placed in outbox.
    Queued,
    /// Same `content_id` was already sent or received.
    DuplicateContentId,
    /// Same text as the last applied/queued clipboard payload (echo / no-op).
    UnchangedText,
    /// Same image as the last applied/queued image payload (echo / no-op).
    UnchangedImage,
    /// Image exceeds inline transport budget.
    ImageTooLarge { byte_len: usize, limit: usize },
}

/// Result of handling an inbound clipboard text message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundClipboardResult {
    /// New text content applied to session state.
    Applied { content_id: String, text: String },
    /// New image content applied to session state.
    AppliedImage {
        content_id: String,
        width: u32,
        height: u32,
        encoding: crate::ImageEncoding,
        data: Vec<u8>,
    },
    /// `content_id` already seen — ignored.
    DuplicateContentId,
}

/// Result of offering / requesting a file transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueFileResult {
    /// Offer or request placed in outbox.
    Queued,
    /// Same `transfer_id` already staged locally.
    DuplicateTransferId,
    /// File exceeds transfer soft cap.
    FileTooLarge { byte_len: u64, limit: u64 },
    /// No matching inbound offer to request.
    UnknownTransferId,
}

/// Result of handling inbound file-channel messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundFileResult {
    /// Peer announced a file available for pull.
    Offered {
        transfer_id: String,
        file_name: String,
        size: u64,
    },
    /// Transfer finished; bytes live on disk at `path` (caller should move/rename).
    Applied {
        transfer_id: String,
        file_name: String,
        path: PathBuf,
        size: u64,
        sha256_hex: String,
    },
    /// Transfer failed or was aborted by peer.
    Failed {
        transfer_id: String,
        message: String,
    },
}

#[derive(Debug)]
enum OutboundBody {
    Memory(Vec<u8>),
    Path(PathBuf),
}

#[derive(Debug)]
struct StagedOutboundFile {
    file_name: String,
    size: u64,
    body: OutboundBody,
    /// Precomputed digest for memory offers; path offers hash while sending.
    sha256_hex: Option<String>,
}

#[derive(Debug)]
struct ActiveOutboundSend {
    transfer_id: String,
    #[allow(dead_code)]
    file_name: String,
    size: u64,
    body: OutboundBody,
    next_offset: u64,
    /// Live hasher when digest was not precomputed.
    hasher: Option<Sha256>,
    sha256_hex: Option<String>,
    file: Option<File>,
}

#[derive(Debug, Clone)]
struct InboundOffer {
    file_name: String,
    size: u64,
    sha256_hex: String,
    #[allow(dead_code)]
    from: DeviceId,
}

struct IncomingFile {
    file_name: String,
    expected_size: u64,
    expected_sha256: String,
    #[allow(dead_code)]
    from: DeviceId,
    part_path: PathBuf,
    writer: File,
    next_offset: u64,
    hasher: Sha256,
}

impl std::fmt::Debug for IncomingFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncomingFile")
            .field("file_name", &self.file_name)
            .field("expected_size", &self.expected_size)
            .field("expected_sha256", &self.expected_sha256)
            .field("part_path", &self.part_path)
            .field("next_offset", &self.next_offset)
            .finish()
    }
}

/// Read-only view of session fields useful for daemon logging / UI later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub local_device: DeviceId,
    pub peer_device: Option<DeviceId>,
    pub state: ConnectionState,
    pub sync_state: SyncState,
    pub protocol_version: u8,
    pub last_heartbeat_seq: u64,
    pub last_clipboard_content_id: Option<String>,
    pub last_clipboard_text: Option<String>,
    pub last_clipboard_image_content_id: Option<String>,
    pub last_clipboard_image_bytes: Option<usize>,
    pub last_file_transfer_id: Option<String>,
    pub last_file_name: Option<String>,
    pub last_file_bytes: Option<u64>,
    pub missed_heartbeat_acks: u32,
    pub outstanding_heartbeat_seq: Option<u64>,
}

/// In-memory 1-on-1 session. Types keep `DeviceId` for future multi-device use.
#[derive(Debug)]
pub struct Session {
    local_device: DeviceId,
    peer_device: Option<DeviceId>,
    state: ConnectionState,
    sync_state: SyncState,
    expected_code: Option<String>,
    last_heartbeat_seq: u64,
    /// Seq of the last heartbeat we sent that has not been acked yet.
    outstanding_heartbeat_seq: Option<u64>,
    /// How many `HeartbeatTick`s fired while a previous heartbeat was still outstanding.
    missed_heartbeat_acks: u32,
    last_clipboard_content_id: Option<String>,
    last_clipboard_text: Option<String>,
    last_clipboard_image_content_id: Option<String>,
    last_clipboard_image_fp: Option<u64>,
    last_clipboard_image_bytes: Option<usize>,
    seen_content_ids: VecDeque<String>,
    /// Filled when an inbound clipboard payload is newly applied (daemon should consume).
    last_inbound_clipboard: Option<InboundClipboardResult>,
    /// Local files staged after offer (path or small memory body).
    staged_outbound_files: HashMap<String, StagedOutboundFile>,
    /// Single in-flight outbound stream (chunks produced via [`Self::pump_outbound_file`]).
    active_outbound: Option<ActiveOutboundSend>,
    /// Offers announced by the peer, keyed by transfer_id.
    inbound_offers: HashMap<String, InboundOffer>,
    /// In-progress receive writing to `.part` files.
    incoming_files: HashMap<String, IncomingFile>,
    /// Directory for inbound `.part` / temp files (defaults to system temp).
    file_receive_dir: Option<PathBuf>,
    /// Filled when an inbound file event is newly observed (daemon/UI should consume).
    last_inbound_file: Option<InboundFileResult>,
    last_file_transfer_id: Option<String>,
    last_file_name: Option<String>,
    last_file_bytes: Option<u64>,
    /// Outbound messages produced by the last `handle` / queue / pump call.
    pending_outbox: Vec<Message>,
}

impl Session {
    pub fn new(local_device: DeviceId) -> Result<Self, ProtocolError> {
        if local_device.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        Ok(Self {
            local_device,
            peer_device: None,
            state: ConnectionState::Disconnected,
            sync_state: SyncState::Idle,
            expected_code: None,
            last_heartbeat_seq: 0,
            outstanding_heartbeat_seq: None,
            missed_heartbeat_acks: 0,
            last_clipboard_content_id: None,
            last_clipboard_text: None,
            last_clipboard_image_content_id: None,
            last_clipboard_image_fp: None,
            last_clipboard_image_bytes: None,
            seen_content_ids: VecDeque::new(),
            last_inbound_clipboard: None,
            staged_outbound_files: HashMap::new(),
            active_outbound: None,
            inbound_offers: HashMap::new(),
            incoming_files: HashMap::new(),
            file_receive_dir: None,
            last_inbound_file: None,
            last_file_transfer_id: None,
            last_file_name: None,
            last_file_bytes: None,
            pending_outbox: Vec::new(),
        })
    }

    /// Directory used for inbound `.part` files (created on demand).
    pub fn set_file_receive_dir(&mut self, dir: impl Into<PathBuf>) {
        self.file_receive_dir = Some(dir.into());
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn sync_state(&self) -> SyncState {
        self.sync_state
    }

    pub fn peer_device(&self) -> Option<&DeviceId> {
        self.peer_device.as_ref()
    }

    pub fn local_device(&self) -> &DeviceId {
        &self.local_device
    }

    pub fn missed_heartbeat_acks(&self) -> u32 {
        self.missed_heartbeat_acks
    }

    /// True when the peer has missed at least `threshold` heartbeat acks.
    pub fn peer_heartbeat_suspect(&self, threshold: u32) -> bool {
        self.state == ConnectionState::Connected && self.missed_heartbeat_acks >= threshold
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            local_device: self.local_device.clone(),
            peer_device: self.peer_device.clone(),
            state: self.state,
            sync_state: self.sync_state,
            protocol_version: PROTOCOL_VERSION,
            last_heartbeat_seq: self.last_heartbeat_seq,
            last_clipboard_content_id: self.last_clipboard_content_id.clone(),
            last_clipboard_text: self.last_clipboard_text.clone(),
            last_clipboard_image_content_id: self.last_clipboard_image_content_id.clone(),
            last_clipboard_image_bytes: self.last_clipboard_image_bytes,
            last_file_transfer_id: self.last_file_transfer_id.clone(),
            last_file_name: self.last_file_name.clone(),
            last_file_bytes: self.last_file_bytes,
            missed_heartbeat_acks: self.missed_heartbeat_acks,
            outstanding_heartbeat_seq: self.outstanding_heartbeat_seq,
        }
    }

    /// Drain messages generated by the previous event (FIFO).
    pub fn take_outbox(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.pending_outbox)
    }

    /// Take the last inbound clipboard apply result (if any).
    pub fn take_inbound_clipboard(&mut self) -> Option<InboundClipboardResult> {
        self.last_inbound_clipboard.take()
    }

    /// Take the last inbound file-channel event (if any).
    pub fn take_inbound_file(&mut self) -> Option<InboundFileResult> {
        self.last_inbound_file.take()
    }

    /// Best-effort receive progress for the first in-flight inbound transfer.
    ///
    /// Returns `(transfer_id, bytes_received, bytes_total)`.
    pub fn inbound_file_progress(&self) -> Option<(String, u64, u64)> {
        if let Some((id, incoming)) = self.incoming_files.iter().next() {
            return Some((id.clone(), incoming.next_offset, incoming.expected_size));
        }
        if let Some((id, offer)) = self.inbound_offers.iter().next() {
            // Offer known but no chunks yet (or empty file not started).
            if self.incoming_files.is_empty() {
                return Some((id.clone(), 0, offer.size));
            }
        }
        None
    }

    /// Best-effort send progress for the active outbound stream.
    pub fn outbound_file_progress(&self) -> Option<(String, u64, u64)> {
        self.active_outbound
            .as_ref()
            .map(|a| (a.transfer_id.clone(), a.next_offset, a.size))
    }

    /// True when more outbound file chunks still need pumping.
    pub fn has_pending_outbound_file(&self) -> bool {
        self.active_outbound.is_some()
    }

    pub fn handle(&mut self, event: SessionEvent) -> Result<(), SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
        self.last_inbound_file = None;
        match event {
            SessionEvent::StartPairing { expected_code } => self.on_start_pairing(expected_code),
            SessionEvent::Message(message) => self.on_message(message),
            SessionEvent::HeartbeatTick => self.on_heartbeat_tick(),
            SessionEvent::Disconnect => {
                self.reset_to_disconnected(true);
                Ok(())
            }
        }
    }

    /// Queue outbound text after connected.
    ///
    /// Dedup policy:
    /// - duplicate `content_id` → [`QueueClipboardResult::DuplicateContentId`] (no outbox)
    /// - same text as last clipboard text → [`QueueClipboardResult::UnchangedText`] (no outbox)
    /// - otherwise queue `ClipboardText` and remember the id
    pub fn queue_clipboard_text(
        &mut self,
        content_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<QueueClipboardResult, SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "queue_clipboard_text",
            });
        }
        let content_id = content_id.into();
        let text = text.into();
        if self.has_seen_content_id(&content_id) {
            return Ok(QueueClipboardResult::DuplicateContentId);
        }
        if self.last_clipboard_text.as_deref() == Some(text.as_str()) {
            return Ok(QueueClipboardResult::UnchangedText);
        }
        let payload =
            crate::ClipboardTextPayload::new(self.local_device.clone(), content_id, text)?;
        self.sync_state = SyncState::Syncing;
        self.remember_content_id(payload.content_id.clone());
        self.last_clipboard_content_id = Some(payload.content_id.clone());
        self.last_clipboard_text = Some(payload.text.clone());
        self.pending_outbox
            .push(Message::clipboard_text(payload));
        self.sync_state = SyncState::Idle;
        Ok(QueueClipboardResult::Queued)
    }

    /// Maximum raw RGBA bytes accepted for inline image sync.
    pub const INLINE_IMAGE_MAX_BYTES: usize = 12 * 1024 * 1024;

    /// Queue outbound raw-RGBA image after connected (tests / tiny images).
    pub fn queue_clipboard_image(
        &mut self,
        content_id: impl Into<String>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<QueueClipboardResult, SessionError> {
        self.queue_clipboard_image_encoded(
            content_id,
            width,
            height,
            crate::ImageEncoding::RawRgba,
            rgba,
        )
    }

    /// Queue outbound image with explicit on-wire encoding (PNG preferred).
    pub fn queue_clipboard_image_encoded(
        &mut self,
        content_id: impl Into<String>,
        width: u32,
        height: u32,
        encoding: crate::ImageEncoding,
        data: Vec<u8>,
    ) -> Result<QueueClipboardResult, SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "queue_clipboard_image",
            });
        }
        let content_id = content_id.into();
        if self.has_seen_content_id(&content_id) {
            return Ok(QueueClipboardResult::DuplicateContentId);
        }
        let fp = image_fingerprint(width, height, encoding.as_u8(), &data);
        if self.last_clipboard_image_fp == Some(fp) {
            return Ok(QueueClipboardResult::UnchangedImage);
        }
        if data.len() > Self::INLINE_IMAGE_MAX_BYTES {
            return Ok(QueueClipboardResult::ImageTooLarge {
                byte_len: data.len(),
                limit: Self::INLINE_IMAGE_MAX_BYTES,
            });
        }
        let payload = crate::ClipboardImagePayload::encoded(
            self.local_device.clone(),
            content_id,
            width,
            height,
            encoding,
            data,
        )?;
        self.sync_state = SyncState::Syncing;
        self.remember_content_id(payload.content_id.clone());
        self.last_clipboard_content_id = Some(payload.content_id.clone());
        self.last_clipboard_image_content_id = Some(payload.content_id.clone());
        self.last_clipboard_image_fp = Some(fp);
        self.last_clipboard_image_bytes = Some(payload.data.len());
        self.pending_outbox
            .push(Message::clipboard_image(payload));
        self.sync_state = SyncState::Idle;
        Ok(QueueClipboardResult::Queued)
    }

    /// Stage file bytes in memory and announce a [`Message::FileOffer`] to the peer.
    ///
    /// Prefer [`Self::offer_file_path`] for large files. This path precomputes SHA-256 and
    /// keeps bytes in RAM up to [`MAX_MEMORY_FILE_BYTES`].
    pub fn offer_file(
        &mut self,
        transfer_id: impl Into<String>,
        file_name: impl Into<String>,
        data: Vec<u8>,
    ) -> Result<QueueFileResult, SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
        self.last_inbound_file = None;
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "offer_file",
            });
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId.into());
        }
        if self.staged_outbound_files.contains_key(&transfer_id) {
            return Ok(QueueFileResult::DuplicateTransferId);
        }
        let byte_len = data.len() as u64;
        if byte_len > MAX_FILE_BYTES {
            return Ok(QueueFileResult::FileTooLarge {
                byte_len,
                limit: MAX_FILE_BYTES,
            });
        }
        if data.len() > MAX_MEMORY_FILE_BYTES {
            return Ok(QueueFileResult::FileTooLarge {
                byte_len,
                limit: MAX_MEMORY_FILE_BYTES as u64,
            });
        }
        let file_name = file_name.into();
        let digest = Sha256::digest(&data);
        let sha256_hex = bytes_to_hex(&digest);
        let offer = FileOfferPayload::with_sha256(
            self.local_device.clone(),
            transfer_id.clone(),
            file_name.clone(),
            byte_len,
            sha256_hex.clone(),
        )?;
        self.staged_outbound_files.insert(
            transfer_id.clone(),
            StagedOutboundFile {
                file_name: file_name.clone(),
                size: byte_len,
                body: OutboundBody::Memory(data),
                sha256_hex: Some(sha256_hex),
            },
        );
        self.sync_state = SyncState::Syncing;
        self.last_file_transfer_id = Some(transfer_id);
        self.last_file_name = Some(file_name);
        self.last_file_bytes = Some(offer.size);
        self.pending_outbox.push(Message::file_offer(offer));
        self.sync_state = SyncState::Idle;
        Ok(QueueFileResult::Queued)
    }

    /// Stage a local file by path (streamed on request; does not load whole file).
    pub fn offer_file_path(
        &mut self,
        transfer_id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<QueueFileResult, SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
        self.last_inbound_file = None;
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "offer_file_path",
            });
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId.into());
        }
        if self.staged_outbound_files.contains_key(&transfer_id) {
            return Ok(QueueFileResult::DuplicateTransferId);
        }
        let path = path.as_ref();
        let meta = fs::metadata(path).map_err(|_| {
            SessionError::Protocol(ProtocolError::InvalidFile("failed to stat file"))
        })?;
        if !meta.is_file() {
            return Err(ProtocolError::InvalidFile("path is not a regular file").into());
        }
        let size = meta.len();
        if size > MAX_FILE_BYTES {
            return Ok(QueueFileResult::FileTooLarge {
                byte_len: size,
                limit: MAX_FILE_BYTES,
            });
        }
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .ok_or(ProtocolError::InvalidFile("file name missing"))?
            .to_string();
        // Offer without pre-hash (hash while streaming) to avoid double full-file read.
        let offer = FileOfferPayload::new(
            self.local_device.clone(),
            transfer_id.clone(),
            file_name.clone(),
            size,
        )?;
        self.staged_outbound_files.insert(
            transfer_id.clone(),
            StagedOutboundFile {
                file_name: file_name.clone(),
                size,
                body: OutboundBody::Path(path.to_path_buf()),
                sha256_hex: None,
            },
        );
        self.sync_state = SyncState::Syncing;
        self.last_file_transfer_id = Some(transfer_id);
        self.last_file_name = Some(file_name);
        self.last_file_bytes = Some(size);
        self.pending_outbox.push(Message::file_offer(offer));
        self.sync_state = SyncState::Idle;
        Ok(QueueFileResult::Queued)
    }

    /// Request bytes for a peer offer previously observed via [`InboundFileResult::Offered`].
    pub fn request_file(
        &mut self,
        transfer_id: impl Into<String>,
    ) -> Result<QueueFileResult, SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
        self.last_inbound_file = None;
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "request_file",
            });
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId.into());
        }
        if !self.inbound_offers.contains_key(&transfer_id) {
            return Ok(QueueFileResult::UnknownTransferId);
        }
        let req = FileRequestPayload::new(self.local_device.clone(), transfer_id)?;
        self.sync_state = SyncState::Syncing;
        self.pending_outbox.push(Message::file_request(req));
        self.sync_state = SyncState::Idle;
        Ok(QueueFileResult::Queued)
    }

    /// Emit up to [`OUTBOUND_CHUNKS_PER_PUMP`] file chunks (or a Complete) for the active send.
    ///
    /// Returns `true` if more pumping is still required. Appends to the outbox without clearing it.
    pub fn pump_outbound_file(&mut self) -> Result<bool, SessionError> {
        if self.active_outbound.is_none() {
            return Ok(false);
        }
        if self.state != ConnectionState::Connected {
            self.abort_active_outbound();
            return Ok(false);
        }
        self.sync_state = SyncState::Syncing;
        self.pump_outbound_file_inner()?;
        self.sync_state = SyncState::Idle;
        Ok(self.active_outbound.is_some())
    }

    fn on_start_pairing(&mut self, expected_code: String) -> Result<(), SessionError> {
        if expected_code.is_empty() {
            return Err(ProtocolError::EmptyPairingCode.into());
        }
        match self.state {
            ConnectionState::Disconnected | ConnectionState::Pairing => {
                self.state = ConnectionState::Pairing;
                self.expected_code = Some(expected_code.clone());
                self.peer_device = None;
                self.sync_state = SyncState::Idle;
                self.outstanding_heartbeat_seq = None;
                self.missed_heartbeat_acks = 0;
                self.pending_outbox.push(Message::hello(
                    self.local_device.clone(),
                    crate::VERSION,
                )?);
                self.pending_outbox.push(Message::pair_request(
                    self.local_device.clone(),
                    expected_code,
                )?);
                Ok(())
            }
            ConnectionState::Connected => Err(SessionError::InvalidTransition {
                from: self.state,
                event: "start_pairing",
            }),
        }
    }

    fn on_heartbeat_tick(&mut self) -> Result<(), SessionError> {
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "heartbeat_tick",
            });
        }
        if self.outstanding_heartbeat_seq.is_some() {
            self.missed_heartbeat_acks = self.missed_heartbeat_acks.saturating_add(1);
        }
        self.last_heartbeat_seq = self.last_heartbeat_seq.saturating_add(1);
        self.outstanding_heartbeat_seq = Some(self.last_heartbeat_seq);
        self.pending_outbox
            .push(Message::heartbeat(self.last_heartbeat_seq));
        Ok(())
    }

    fn on_message(&mut self, message: Message) -> Result<(), SessionError> {
        match message {
            Message::Hello { device_id, .. } => self.on_hello(device_id),
            Message::HelloAck { device_id, .. } => self.on_hello_ack(device_id),
            Message::PairRequest {
                device_id,
                pairing_code,
            } => self.on_pair_request(device_id, pairing_code),
            Message::PairAccept { device_id } => self.on_pair_accept(device_id),
            Message::PairReject { reason, .. } => {
                self.reset_to_disconnected(true);
                Err(SessionError::PairRejected(reason))
            }
            Message::Heartbeat { seq } => {
                if self.state != ConnectionState::Connected {
                    return Err(SessionError::InvalidTransition {
                        from: self.state,
                        event: "heartbeat",
                    });
                }
                self.pending_outbox.push(Message::heartbeat_ack(seq));
                Ok(())
            }
            Message::HeartbeatAck { seq } => {
                if self.state != ConnectionState::Connected {
                    return Err(SessionError::InvalidTransition {
                        from: self.state,
                        event: "heartbeat_ack",
                    });
                }
                if self.outstanding_heartbeat_seq == Some(seq)
                    || self.outstanding_heartbeat_seq.is_some()
                {
                    // Accept ack for outstanding seq; also accept newer acks leniently.
                    if self.outstanding_heartbeat_seq == Some(seq)
                        || self
                            .outstanding_heartbeat_seq
                            .is_some_and(|pending| seq >= pending)
                    {
                        self.outstanding_heartbeat_seq = None;
                        self.missed_heartbeat_acks = 0;
                    }
                }
                self.last_heartbeat_seq = self.last_heartbeat_seq.max(seq);
                Ok(())
            }
            Message::ClipboardText(payload) => self.on_clipboard_text(payload),
            Message::ClipboardImage(payload) => self.on_clipboard_image(payload),
            Message::FileOffer(payload) => self.on_file_offer(payload),
            Message::FileRequest(payload) => self.on_file_request(payload),
            Message::FileChunk(payload) => self.on_file_chunk(payload),
            Message::FileComplete(payload) => self.on_file_complete(payload),
            Message::Goodbye { .. } => {
                self.reset_to_disconnected(true);
                Ok(())
            }
        }
    }

    fn on_hello(&mut self, device_id: DeviceId) -> Result<(), SessionError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId.into());
        }
        match self.state {
            ConnectionState::Disconnected => {
                self.state = ConnectionState::Pairing;
                self.peer_device = Some(device_id);
                self.pending_outbox.push(Message::hello_ack(
                    self.local_device.clone(),
                    crate::VERSION,
                )?);
                Ok(())
            }
            ConnectionState::Pairing => {
                self.remember_peer(device_id)?;
                self.pending_outbox.push(Message::hello_ack(
                    self.local_device.clone(),
                    crate::VERSION,
                )?);
                Ok(())
            }
            ConnectionState::Connected => Err(SessionError::InvalidTransition {
                from: self.state,
                event: "hello",
            }),
        }
    }

    fn on_hello_ack(&mut self, device_id: DeviceId) -> Result<(), SessionError> {
        if self.state != ConnectionState::Pairing {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "hello_ack",
            });
        }
        self.remember_peer(device_id)
    }

    fn on_pair_request(
        &mut self,
        device_id: DeviceId,
        pairing_code: String,
    ) -> Result<(), SessionError> {
        if pairing_code.is_empty() {
            return Err(ProtocolError::EmptyPairingCode.into());
        }
        if self.state == ConnectionState::Disconnected {
            self.state = ConnectionState::Pairing;
        }
        if self.state != ConnectionState::Pairing {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "pair_request",
            });
        }
        self.remember_peer(device_id.clone())?;

        match &self.expected_code {
            Some(expected) if expected == &pairing_code => {
                self.state = ConnectionState::Connected;
                self.sync_state = SyncState::Idle;
                self.missed_heartbeat_acks = 0;
                self.outstanding_heartbeat_seq = None;
                self.pending_outbox
                    .push(Message::pair_accept(self.local_device.clone())?);
                Ok(())
            }
            Some(_) => {
                self.pending_outbox.push(Message::pair_reject(
                    self.local_device.clone(),
                    "pairing code mismatch",
                )?);
                self.reset_to_disconnected(false);
                Ok(())
            }
            None => {
                self.expected_code = Some(pairing_code);
                self.state = ConnectionState::Connected;
                self.missed_heartbeat_acks = 0;
                self.outstanding_heartbeat_seq = None;
                self.pending_outbox
                    .push(Message::pair_accept(self.local_device.clone())?);
                Ok(())
            }
        }
    }

    fn on_pair_accept(&mut self, device_id: DeviceId) -> Result<(), SessionError> {
        if self.state != ConnectionState::Pairing {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "pair_accept",
            });
        }
        self.remember_peer(device_id)?;
        self.state = ConnectionState::Connected;
        self.sync_state = SyncState::Idle;
        self.missed_heartbeat_acks = 0;
        self.outstanding_heartbeat_seq = None;
        Ok(())
    }

    fn on_clipboard_text(
        &mut self,
        payload: crate::ClipboardTextPayload,
    ) -> Result<(), SessionError> {
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "clipboard_text",
            });
        }
        if let Some(peer) = &self.peer_device {
            if peer != &payload.device_id {
                return Err(SessionError::UnexpectedPeer(payload.device_id.to_string()));
            }
        } else {
            self.peer_device = Some(payload.device_id.clone());
        }

        if self.has_seen_content_id(&payload.content_id) {
            self.last_inbound_clipboard = Some(InboundClipboardResult::DuplicateContentId);
            return Ok(());
        }

        self.sync_state = SyncState::Syncing;
        self.remember_content_id(payload.content_id.clone());
        self.last_clipboard_content_id = Some(payload.content_id.clone());
        self.last_clipboard_text = Some(payload.text.clone());
        self.last_inbound_clipboard = Some(InboundClipboardResult::Applied {
            content_id: payload.content_id,
            text: payload.text,
        });
        self.sync_state = SyncState::Idle;
        Ok(())
    }

    fn on_clipboard_image(
        &mut self,
        payload: crate::ClipboardImagePayload,
    ) -> Result<(), SessionError> {
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event: "clipboard_image",
            });
        }
        if let Some(peer) = &self.peer_device {
            if peer != &payload.device_id {
                return Err(SessionError::UnexpectedPeer(payload.device_id.to_string()));
            }
        } else {
            self.peer_device = Some(payload.device_id.clone());
        }

        if self.has_seen_content_id(&payload.content_id) {
            self.last_inbound_clipboard = Some(InboundClipboardResult::DuplicateContentId);
            return Ok(());
        }
        if payload.data.len() > Self::INLINE_IMAGE_MAX_BYTES {
            return Err(SessionError::Protocol(ProtocolError::InvalidImage(
                "image exceeds inline limit",
            )));
        }

        self.sync_state = SyncState::Syncing;
        self.remember_content_id(payload.content_id.clone());
        self.last_clipboard_content_id = Some(payload.content_id.clone());
        self.last_clipboard_image_content_id = Some(payload.content_id.clone());
        let fp = image_fingerprint(
            payload.width,
            payload.height,
            payload.encoding.as_u8(),
            &payload.data,
        );
        self.last_clipboard_image_fp = Some(fp);
        self.last_clipboard_image_bytes = Some(payload.data.len());
        self.last_inbound_clipboard = Some(InboundClipboardResult::AppliedImage {
            content_id: payload.content_id,
            width: payload.width,
            height: payload.height,
            encoding: payload.encoding,
            data: payload.data,
        });
        self.sync_state = SyncState::Idle;
        Ok(())
    }

    fn ensure_connected_file_peer(
        &mut self,
        device_id: &DeviceId,
        event: &'static str,
    ) -> Result<(), SessionError> {
        if self.state != ConnectionState::Connected {
            return Err(SessionError::InvalidTransition {
                from: self.state,
                event,
            });
        }
        if let Some(peer) = &self.peer_device {
            if peer != device_id {
                return Err(SessionError::UnexpectedPeer(device_id.to_string()));
            }
        } else {
            self.peer_device = Some(device_id.clone());
        }
        Ok(())
    }

    fn on_file_offer(&mut self, payload: FileOfferPayload) -> Result<(), SessionError> {
        self.ensure_connected_file_peer(&payload.device_id, "file_offer")?;
        if payload.size > MAX_FILE_BYTES {
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: format!(
                    "offer too large: {} > limit {}",
                    payload.size, MAX_FILE_BYTES
                ),
            });
            return Ok(());
        }
        self.inbound_offers.insert(
            payload.transfer_id.clone(),
            InboundOffer {
                file_name: payload.file_name.clone(),
                size: payload.size,
                sha256_hex: payload.sha256_hex,
                from: payload.device_id,
            },
        );
        self.last_inbound_file = Some(InboundFileResult::Offered {
            transfer_id: payload.transfer_id,
            file_name: payload.file_name,
            size: payload.size,
        });
        Ok(())
    }

    fn on_file_request(&mut self, payload: FileRequestPayload) -> Result<(), SessionError> {
        self.ensure_connected_file_peer(&payload.device_id, "file_request")?;
        if self.active_outbound.is_some() {
            let complete = FileCompletePayload::new(
                self.local_device.clone(),
                payload.transfer_id,
                false,
                "sender busy with another transfer",
            )?;
            self.pending_outbox.push(Message::file_complete(complete));
            return Ok(());
        }
        let Some(staged) = self.staged_outbound_files.remove(&payload.transfer_id) else {
            let complete = FileCompletePayload::new(
                self.local_device.clone(),
                payload.transfer_id,
                false,
                "unknown transfer id",
            )?;
            self.pending_outbox.push(Message::file_complete(complete));
            return Ok(());
        };

        let hasher = if staged.sha256_hex.is_some() {
            None
        } else {
            Some(Sha256::new())
        };
        let file = match &staged.body {
            OutboundBody::Path(path) => Some(File::open(path).map_err(|_| {
                SessionError::Protocol(ProtocolError::InvalidFile("failed to open file for send"))
            })?),
            OutboundBody::Memory(_) => None,
        };

        self.active_outbound = Some(ActiveOutboundSend {
            transfer_id: payload.transfer_id,
            file_name: staged.file_name,
            size: staged.size,
            body: staged.body,
            next_offset: 0,
            hasher,
            sha256_hex: staged.sha256_hex,
            file,
        });
        self.sync_state = SyncState::Syncing;
        self.pump_outbound_file_inner()?;
        self.sync_state = SyncState::Idle;
        Ok(())
    }

    fn pump_outbound_file_inner(&mut self) -> Result<(), SessionError> {
        for _ in 0..OUTBOUND_CHUNKS_PER_PUMP {
            let Some(active) = self.active_outbound.as_mut() else {
                return Ok(());
            };
            if active.next_offset >= active.size {
                break;
            }
            let remaining = (active.size - active.next_offset) as usize;
            let take = remaining.min(FILE_CHUNK_SIZE);
            let offset = active.next_offset;
            let mut buf = vec![0u8; take];
            match &mut active.body {
                OutboundBody::Memory(data) => {
                    let start = offset as usize;
                    buf.copy_from_slice(&data[start..start + take]);
                }
                OutboundBody::Path(_) => {
                    let file = active.file.as_mut().ok_or({
                        SessionError::Protocol(ProtocolError::InvalidFile("missing send file handle"))
                    })?;
                    file.seek(SeekFrom::Start(offset)).map_err(|_| {
                        SessionError::Protocol(ProtocolError::InvalidFile("seek failed while sending"))
                    })?;
                    file.read_exact(&mut buf).map_err(|_| {
                        SessionError::Protocol(ProtocolError::InvalidFile("read failed while sending"))
                    })?;
                }
            }
            if let Some(hasher) = active.hasher.as_mut() {
                hasher.update(&buf);
            }
            let chunk = FileChunkPayload::new(
                self.local_device.clone(),
                active.transfer_id.clone(),
                offset,
                buf,
            )?;
            active.next_offset = offset + take as u64;
            self.pending_outbox.push(Message::file_chunk(chunk));
        }

        let finished = self
            .active_outbound
            .as_ref()
            .is_some_and(|a| a.next_offset >= a.size);
        if !finished {
            return Ok(());
        }

        let active = self.active_outbound.take().expect("active outbound");
        let sha256_hex = if let Some(hex) = active.sha256_hex {
            hex
        } else if let Some(hasher) = active.hasher {
            bytes_to_hex(&hasher.finalize())
        } else {
            String::new()
        };
        let complete = FileCompletePayload::with_sha256(
            self.local_device.clone(),
            active.transfer_id,
            true,
            "",
            sha256_hex,
        )?;
        self.pending_outbox.push(Message::file_complete(complete));
        self.sync_state = SyncState::Idle;
        Ok(())
    }

    fn on_file_chunk(&mut self, payload: FileChunkPayload) -> Result<(), SessionError> {
        self.ensure_connected_file_peer(&payload.device_id, "file_chunk")?;
        if self.incoming_files.contains_key(&payload.transfer_id) {
            let fail_msg = {
                let incoming = self.incoming_files.get_mut(&payload.transfer_id).unwrap();
                if payload.offset != incoming.next_offset {
                    Some(format!(
                        "offset mismatch: got {} expected {}",
                        payload.offset, incoming.next_offset
                    ))
                } else {
                    let new_len = incoming
                        .next_offset
                        .saturating_add(payload.data.len() as u64);
                    if new_len > incoming.expected_size || new_len > MAX_FILE_BYTES {
                        Some("chunk exceeds expected size".into())
                    } else if incoming.writer.write_all(&payload.data).is_err() {
                        Some("failed to write part file".into())
                    } else {
                        incoming.hasher.update(&payload.data);
                        incoming.next_offset = new_len;
                        None
                    }
                }
            };
            if let Some(msg) = fail_msg {
                self.fail_incoming(&payload.transfer_id, msg);
            }
            return Ok(());
        }

        let Some(offer) = self.inbound_offers.remove(&payload.transfer_id) else {
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: "chunk for unknown transfer".into(),
            });
            return Ok(());
        };
        if payload.offset != 0 {
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: "first chunk offset must be 0".into(),
            });
            return Ok(());
        }
        if payload.data.len() as u64 > offer.size || offer.size > MAX_FILE_BYTES {
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: "chunk exceeds offer size".into(),
            });
            return Ok(());
        }
        match self.open_incoming(&payload.transfer_id, &offer, &payload.device_id) {
            Ok(mut incoming) => {
                if incoming.writer.write_all(&payload.data).is_err() {
                    let _ = fs::remove_file(&incoming.part_path);
                    self.last_inbound_file = Some(InboundFileResult::Failed {
                        transfer_id: payload.transfer_id,
                        message: "failed to write part file".into(),
                    });
                    return Ok(());
                }
                incoming.hasher.update(&payload.data);
                incoming.next_offset = payload.data.len() as u64;
                self.incoming_files
                    .insert(payload.transfer_id, incoming);
            }
            Err(msg) => {
                self.last_inbound_file = Some(InboundFileResult::Failed {
                    transfer_id: payload.transfer_id,
                    message: msg,
                });
            }
        }
        Ok(())
    }

    fn on_file_complete(&mut self, payload: FileCompletePayload) -> Result<(), SessionError> {
        self.ensure_connected_file_peer(&payload.device_id, "file_complete")?;
        if !payload.ok {
            self.cleanup_incoming(&payload.transfer_id);
            self.inbound_offers.remove(&payload.transfer_id);
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: if payload.message.is_empty() {
                    "transfer failed".into()
                } else {
                    payload.message
                },
            });
            return Ok(());
        }

        // Empty file: complete may arrive with no chunks.
        if !self.incoming_files.contains_key(&payload.transfer_id) {
            if let Some(offer) = self.inbound_offers.remove(&payload.transfer_id) {
                if offer.size == 0 {
                    match self.open_incoming(&payload.transfer_id, &offer, &payload.device_id) {
                        Ok(incoming) => {
                            self.incoming_files
                                .insert(payload.transfer_id.clone(), incoming);
                        }
                        Err(msg) => {
                            self.last_inbound_file = Some(InboundFileResult::Failed {
                                transfer_id: payload.transfer_id,
                                message: msg,
                            });
                            return Ok(());
                        }
                    }
                } else {
                    self.last_inbound_file = Some(InboundFileResult::Failed {
                        transfer_id: payload.transfer_id,
                        message: "complete without data".into(),
                    });
                    return Ok(());
                }
            }
        }

        let Some(mut incoming) = self.incoming_files.remove(&payload.transfer_id) else {
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: "complete for unknown transfer".into(),
            });
            return Ok(());
        };
        self.inbound_offers.remove(&payload.transfer_id);

        if let Err(err) = incoming.writer.flush() {
            drop(incoming.writer);
            let _ = fs::remove_file(&incoming.part_path);
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: format!("flush part file: {err}"),
            });
            return Ok(());
        }
        drop(incoming.writer);

        if incoming.next_offset != incoming.expected_size {
            let _ = fs::remove_file(&incoming.part_path);
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: format!(
                    "size mismatch: got {} expected {}",
                    incoming.next_offset, incoming.expected_size
                ),
            });
            return Ok(());
        }

        let digest = bytes_to_hex(&incoming.hasher.finalize());
        if !payload.sha256_hex.is_empty() && payload.sha256_hex != digest {
            let _ = fs::remove_file(&incoming.part_path);
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: format!(
                    "sha256 mismatch: got {digest} expected {}",
                    payload.sha256_hex
                ),
            });
            return Ok(());
        }
        if !incoming.expected_sha256.is_empty() && incoming.expected_sha256 != digest {
            let _ = fs::remove_file(&incoming.part_path);
            self.last_inbound_file = Some(InboundFileResult::Failed {
                transfer_id: payload.transfer_id,
                message: format!(
                    "sha256 mismatch vs offer: got {digest} expected {}",
                    incoming.expected_sha256
                ),
            });
            return Ok(());
        }

        self.last_file_transfer_id = Some(payload.transfer_id.clone());
        self.last_file_name = Some(incoming.file_name.clone());
        self.last_file_bytes = Some(incoming.expected_size);
        self.last_inbound_file = Some(InboundFileResult::Applied {
            transfer_id: payload.transfer_id,
            file_name: incoming.file_name,
            path: incoming.part_path,
            size: incoming.expected_size,
            sha256_hex: digest,
        });
        self.sync_state = SyncState::Idle;
        Ok(())
    }

    fn open_incoming(
        &self,
        transfer_id: &str,
        offer: &InboundOffer,
        from: &DeviceId,
    ) -> Result<IncomingFile, String> {
        let dir = self.receive_dir();
        fs::create_dir_all(&dir).map_err(|e| format!("create receive dir: {e}"))?;
        let part_path = dir.join(format!("{transfer_id}.part"));
        if part_path.exists() {
            let _ = fs::remove_file(&part_path);
        }
        let writer = File::create(&part_path).map_err(|e| format!("create part file: {e}"))?;
        Ok(IncomingFile {
            file_name: offer.file_name.clone(),
            expected_size: offer.size,
            expected_sha256: offer.sha256_hex.clone(),
            from: from.clone(),
            part_path,
            writer,
            next_offset: 0,
            hasher: Sha256::new(),
        })
    }

    fn receive_dir(&self) -> PathBuf {
        self.file_receive_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("m590-incoming"))
    }

    fn fail_incoming(&mut self, transfer_id: &str, message: String) {
        self.cleanup_incoming(transfer_id);
        self.inbound_offers.remove(transfer_id);
        self.last_inbound_file = Some(InboundFileResult::Failed {
            transfer_id: transfer_id.to_string(),
            message,
        });
    }

    fn cleanup_incoming(&mut self, transfer_id: &str) {
        if let Some(incoming) = self.incoming_files.remove(transfer_id) {
            drop(incoming.writer);
            let _ = fs::remove_file(&incoming.part_path);
        }
    }

    fn abort_active_outbound(&mut self) {
        self.active_outbound = None;
    }

    fn cleanup_all_incoming_parts(&mut self) {
        let ids: Vec<String> = self.incoming_files.keys().cloned().collect();
        for id in ids {
            self.cleanup_incoming(&id);
        }
    }

    fn remember_peer(&mut self, device_id: DeviceId) -> Result<(), SessionError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId.into());
        }
        if device_id == self.local_device {
            return Err(SessionError::UnexpectedPeer(device_id.to_string()));
        }
        match &self.peer_device {
            Some(existing) if existing != &device_id => {
                Err(SessionError::UnexpectedPeer(device_id.to_string()))
            }
            Some(_) => Ok(()),
            None => {
                self.peer_device = Some(device_id);
                Ok(())
            }
        }
    }

    fn has_seen_content_id(&self, content_id: &str) -> bool {
        self.seen_content_ids.iter().any(|id| id == content_id)
    }

    fn remember_content_id(&mut self, content_id: String) {
        if self.has_seen_content_id(&content_id) {
            return;
        }
        if self.seen_content_ids.len() >= SEEN_CONTENT_ID_CAP {
            self.seen_content_ids.pop_front();
        }
        self.seen_content_ids.push_back(content_id);
    }

    fn reset_to_disconnected(&mut self, clear_outbox: bool) {
        self.state = ConnectionState::Disconnected;
        self.sync_state = SyncState::Idle;
        self.peer_device = None;
        self.expected_code = None;
        self.last_heartbeat_seq = 0;
        self.outstanding_heartbeat_seq = None;
        self.missed_heartbeat_acks = 0;
        self.last_clipboard_content_id = None;
        self.last_clipboard_text = None;
        self.last_clipboard_image_content_id = None;
        self.last_clipboard_image_fp = None;
        self.last_clipboard_image_bytes = None;
        self.seen_content_ids.clear();
        self.last_inbound_clipboard = None;
        self.staged_outbound_files.clear();
        self.abort_active_outbound();
        self.inbound_offers.clear();
        self.cleanup_all_incoming_parts();
        self.last_inbound_file = None;
        self.last_file_transfer_id = None;
        self.last_file_name = None;
        self.last_file_bytes = None;
        if clear_outbox {
            self.pending_outbox.clear();
        }
    }
}

fn image_fingerprint(width: u32, height: u32, encoding: u8, data: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    encoding.hash(&mut hasher);
    data.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClipboardTextPayload;

    fn exchange(a: &mut Session, b: &mut Session) {
        use std::collections::VecDeque;

        let mut qa: VecDeque<Message> = a.take_outbox().into();
        let mut qb: VecDeque<Message> = b.take_outbox().into();
        let mut guard = 0;
        while !qa.is_empty() || !qb.is_empty() {
            guard += 1;
            assert!(guard < 64, "message exchange did not settle");

            if let Some(msg) = qa.pop_front() {
                b.handle(SessionEvent::Message(msg)).unwrap();
                qb.extend(b.take_outbox());
            }
            if let Some(msg) = qb.pop_front() {
                a.handle(SessionEvent::Message(msg)).unwrap();
                qa.extend(a.take_outbox());
            }
        }
    }

    fn pair_host_joiner() -> (Session, Session) {
        let mut host = Session::new(DeviceId::new("host")).unwrap();
        host.handle(SessionEvent::StartPairing {
            expected_code: "123456".into(),
        })
        .unwrap();
        let _ = host.take_outbox();

        let mut joiner = Session::new(DeviceId::new("joiner")).unwrap();
        joiner
            .handle(SessionEvent::StartPairing {
                expected_code: "123456".into(),
            })
            .unwrap();
        exchange(&mut host, &mut joiner);
        assert_eq!(host.state(), ConnectionState::Connected);
        assert_eq!(joiner.state(), ConnectionState::Connected);
        (host, joiner)
    }

    #[test]
    fn pairing_happy_path_disconnected_to_connected() {
        let (host, joiner) = pair_host_joiner();
        assert_eq!(host.peer_device().map(DeviceId::as_str), Some("joiner"));
        assert_eq!(joiner.peer_device().map(DeviceId::as_str), Some("host"));
    }

    #[test]
    fn pair_code_mismatch_sends_reject_and_disconnects() {
        let mut host = Session::new(DeviceId::new("host")).unwrap();
        host.handle(SessionEvent::StartPairing {
            expected_code: "111111".into(),
        })
        .unwrap();
        let _ = host.take_outbox();

        host.handle(SessionEvent::Message(
            Message::pair_request(DeviceId::new("joiner"), "222222").unwrap(),
        ))
        .unwrap();

        let out = host.take_outbox();
        assert!(matches!(
            out.as_slice(),
            [Message::PairReject { reason, .. }] if reason == "pairing code mismatch"
        ));
        assert_eq!(host.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn pair_reject_message_surfaces_error() {
        let mut joiner = Session::new(DeviceId::new("joiner")).unwrap();
        joiner
            .handle(SessionEvent::StartPairing {
                expected_code: "111111".into(),
            })
            .unwrap();
        let _ = joiner.take_outbox();
        let err = joiner
            .handle(SessionEvent::Message(
                Message::pair_reject(DeviceId::new("host"), "pairing code mismatch").unwrap(),
            ))
            .unwrap_err();
        assert!(matches!(
            err,
            SessionError::PairRejected(reason) if reason == "pairing code mismatch"
        ));
        assert_eq!(joiner.state(), ConnectionState::Disconnected);
    }

    #[test]
    fn heartbeat_and_clipboard_only_when_connected() {
        let mut session = Session::new(DeviceId::new("a")).unwrap();
        assert!(session.handle(SessionEvent::HeartbeatTick).is_err());

        session
            .handle(SessionEvent::StartPairing {
                expected_code: "123456".into(),
            })
            .unwrap();
        let _ = session.take_outbox();
        session
            .handle(SessionEvent::Message(
                Message::pair_accept(DeviceId::new("b")).unwrap(),
            ))
            .unwrap();
        assert_eq!(session.state(), ConnectionState::Connected);

        session.handle(SessionEvent::HeartbeatTick).unwrap();
        let hb = session.take_outbox();
        assert!(matches!(hb.as_slice(), [Message::Heartbeat { seq: 1 }]));

        let queued = session
            .queue_clipboard_text("c1", "hello from a")
            .unwrap();
        assert_eq!(queued, QueueClipboardResult::Queued);
        let clip = session.take_outbox();
        assert!(matches!(
            clip.as_slice(),
            [Message::ClipboardText(ClipboardTextPayload { text, .. })] if text == "hello from a"
        ));
    }

    #[test]
    fn heartbeat_miss_counter_and_ack_clear() {
        let (mut host, _joiner) = pair_host_joiner();
        host.handle(SessionEvent::HeartbeatTick).unwrap();
        assert_eq!(host.missed_heartbeat_acks(), 0);
        assert_eq!(host.snapshot().outstanding_heartbeat_seq, Some(1));

        // Second tick without ack → miss++
        host.handle(SessionEvent::HeartbeatTick).unwrap();
        assert_eq!(host.missed_heartbeat_acks(), 1);
        host.handle(SessionEvent::HeartbeatTick).unwrap();
        assert_eq!(host.missed_heartbeat_acks(), 2);
        assert!(host.peer_heartbeat_suspect(2));

        host.handle(SessionEvent::Message(Message::heartbeat_ack(3)))
            .unwrap();
        assert_eq!(host.missed_heartbeat_acks(), 0);
        assert!(host.snapshot().outstanding_heartbeat_seq.is_none());
        assert!(!host.peer_heartbeat_suspect(DEFAULT_HEARTBEAT_MISS_THRESHOLD));
    }

    #[test]
    fn clipboard_content_id_dedup_send_and_receive() {
        let (mut host, mut joiner) = pair_host_joiner();

        assert_eq!(
            joiner.queue_clipboard_text("cid-1", "hello").unwrap(),
            QueueClipboardResult::Queued
        );
        let msg = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(msg)).unwrap();
        assert_eq!(
            host.take_inbound_clipboard(),
            Some(InboundClipboardResult::Applied {
                content_id: "cid-1".into(),
                text: "hello".into(),
            })
        );

        // Re-deliver same content id → duplicate, state unchanged text still hello
        let dup = Message::clipboard_text(
            ClipboardTextPayload::new(DeviceId::new("joiner"), "cid-1", "hello-again").unwrap(),
        );
        host.handle(SessionEvent::Message(dup)).unwrap();
        assert_eq!(
            host.take_inbound_clipboard(),
            Some(InboundClipboardResult::DuplicateContentId)
        );
        assert_eq!(host.snapshot().last_clipboard_text.as_deref(), Some("hello"));

        // Outbound duplicate id
        assert_eq!(
            joiner.queue_clipboard_text("cid-1", "other").unwrap(),
            QueueClipboardResult::DuplicateContentId
        );
        assert!(joiner.take_outbox().is_empty());

        // Unchanged text echo suppression
        assert_eq!(
            joiner.queue_clipboard_text("cid-2", "hello").unwrap(),
            QueueClipboardResult::UnchangedText
        );
        // joiner last text is hello from first queue
        assert!(joiner.take_outbox().is_empty());
    }

    #[test]
    fn disconnect_resets_state() {
        let mut session = Session::new(DeviceId::new("a")).unwrap();
        session
            .handle(SessionEvent::StartPairing {
                expected_code: "1".into(),
            })
            .unwrap();
        session.handle(SessionEvent::Disconnect).unwrap();
        assert_eq!(session.state(), ConnectionState::Disconnected);
        assert!(session.peer_device().is_none());
        assert_eq!(session.missed_heartbeat_acks(), 0);
    }

    #[test]
    fn disconnect_removes_incoming_part_file() {
        let (mut host, mut joiner) = pair_host_joiner();
        let dir = std::env::temp_dir().join(format!(
            "m590-disconnect-part-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        host.set_file_receive_dir(&dir);
        let data = vec![7u8; FILE_CHUNK_SIZE * (OUTBOUND_CHUNKS_PER_PUMP + 1)];
        assert_eq!(
            joiner
                .offer_file("cleanup-part", "partial.bin", data)
                .unwrap(),
            QueueFileResult::Queued
        );
        let offer = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(offer)).unwrap();
        let _ = host.take_inbound_file();
        host.request_file("cleanup-part").unwrap();
        let request = host.take_outbox().pop().unwrap();
        joiner.handle(SessionEvent::Message(request)).unwrap();
        let first_chunk = joiner
            .take_outbox()
            .into_iter()
            .find(|message| matches!(message, Message::FileChunk(_)))
            .unwrap();
        host.handle(SessionEvent::Message(first_chunk)).unwrap();

        let part = dir.join("cleanup-part.part");
        assert!(part.exists());
        host.handle(SessionEvent::Disconnect).unwrap();
        assert!(!part.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn snapshot_exposes_protocol_version() {
        let session = Session::new(DeviceId::new("a")).unwrap();
        assert_eq!(session.snapshot().protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn clipboard_image_dedup_and_apply() {
        let (mut host, mut joiner) = pair_host_joiner();
        let rgba = vec![9, 8, 7, 255];
        assert_eq!(
            joiner
                .queue_clipboard_image("img-1", 1, 1, rgba.clone())
                .unwrap(),
            QueueClipboardResult::Queued
        );
        let msg = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(msg)).unwrap();
        assert_eq!(
            host.take_inbound_clipboard(),
            Some(InboundClipboardResult::AppliedImage {
                content_id: "img-1".into(),
                width: 1,
                height: 1,
                encoding: crate::ImageEncoding::RawRgba,
                data: rgba.clone(),
            })
        );
        assert_eq!(
            joiner
                .queue_clipboard_image("img-1", 1, 1, rgba.clone())
                .unwrap(),
            QueueClipboardResult::DuplicateContentId
        );
        assert_eq!(
            joiner
                .queue_clipboard_image("img-2", 1, 1, rgba)
                .unwrap(),
            QueueClipboardResult::UnchangedImage
        );
    }

    fn drain_file_send(sender: &mut Session, receiver: &mut Session) {
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 100_000, "file send did not finish");
            let out = sender.take_outbox();
            for msg in out {
                receiver.handle(SessionEvent::Message(msg)).unwrap();
            }
            if sender.has_pending_outbound_file() {
                sender.pump_outbound_file().unwrap();
                continue;
            }
            break;
        }
    }

    #[test]
    fn file_offer_request_chunk_complete_small_file() {
        let (mut host, mut joiner) = pair_host_joiner();
        // Multi-chunk: slightly over one FILE_CHUNK_SIZE.
        let mut data = vec![0u8; FILE_CHUNK_SIZE + 100];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let expect_sha = crate::bytes_to_hex(&{
            use sha2::Digest;
            sha2::Sha256::digest(&data)
        });

        assert_eq!(
            joiner
                .offer_file("xfer-1", "note.bin", data.clone())
                .unwrap(),
            QueueFileResult::Queued
        );
        let offer_msg = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(offer_msg)).unwrap();
        assert_eq!(
            host.take_inbound_file(),
            Some(InboundFileResult::Offered {
                transfer_id: "xfer-1".into(),
                file_name: "note.bin".into(),
                size: data.len() as u64,
            })
        );

        assert_eq!(
            host.request_file("xfer-1").unwrap(),
            QueueFileResult::Queued
        );
        let req_msg = host.take_outbox().pop().unwrap();
        joiner.handle(SessionEvent::Message(req_msg)).unwrap();
        drain_file_send(&mut joiner, &mut host);

        let Some(InboundFileResult::Applied {
            transfer_id,
            file_name,
            path,
            size,
            sha256_hex,
        }) = host.take_inbound_file()
        else {
            panic!("expected applied file");
        };
        assert_eq!(transfer_id, "xfer-1");
        assert_eq!(file_name, "note.bin");
        assert_eq!(size, data.len() as u64);
        assert_eq!(sha256_hex, expect_sha);
        assert_eq!(fs::read(&path).unwrap(), data);
        assert_eq!(host.snapshot().last_file_name.as_deref(), Some("note.bin"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn file_empty_completes_without_chunks() {
        let (mut host, mut joiner) = pair_host_joiner();
        assert_eq!(
            joiner.offer_file("empty-1", "empty.txt", Vec::new()).unwrap(),
            QueueFileResult::Queued
        );
        let offer_msg = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(offer_msg)).unwrap();
        assert!(matches!(
            host.take_inbound_file(),
            Some(InboundFileResult::Offered { size: 0, .. })
        ));
        host.request_file("empty-1").unwrap();
        let req = host.take_outbox().pop().unwrap();
        joiner.handle(SessionEvent::Message(req)).unwrap();
        let out = joiner.take_outbox();
        assert_eq!(out.len(), 1, "empty file should only send complete");
        for msg in out {
            host.handle(SessionEvent::Message(msg)).unwrap();
        }
        let Some(InboundFileResult::Applied {
            transfer_id,
            file_name,
            path,
            size,
            sha256_hex,
        }) = host.take_inbound_file()
        else {
            panic!("expected applied empty file");
        };
        assert_eq!(transfer_id, "empty-1");
        assert_eq!(file_name, "empty.txt");
        assert_eq!(size, 0);
        assert_eq!(fs::read(&path).unwrap(), b"");
        assert_eq!(
            sha256_hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn file_path_offer_streams_without_loading_all() {
        let (mut host, mut joiner) = pair_host_joiner();
        let dir = std::env::temp_dir().join(format!(
            "m590-stream-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        host.set_file_receive_dir(dir.join("recv"));
        let src = dir.join("big.bin");
        let n = OUTBOUND_CHUNKS_PER_PUMP * FILE_CHUNK_SIZE + 1234;
        let mut data = vec![0u8; n];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 199) as u8;
        }
        fs::write(&src, &data).unwrap();

        assert_eq!(
            joiner.offer_file_path("path-1", &src).unwrap(),
            QueueFileResult::Queued
        );
        let offer_msg = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(offer_msg)).unwrap();
        assert!(matches!(
            host.take_inbound_file(),
            Some(InboundFileResult::Offered {
                ref transfer_id,
                size,
                ..
            }) if transfer_id == "path-1" && size == n as u64
        ));
        host.request_file("path-1").unwrap();
        let req = host.take_outbox().pop().unwrap();
        joiner.handle(SessionEvent::Message(req)).unwrap();
        assert!(joiner.has_pending_outbound_file());
        drain_file_send(&mut joiner, &mut host);
        let Some(InboundFileResult::Applied {
            path,
            size,
            sha256_hex,
            ..
        }) = host.take_inbound_file()
        else {
            panic!("expected applied path file");
        };
        assert_eq!(size, n as u64);
        assert_eq!(fs::read(&path).unwrap(), data);
        let expect_sha = crate::bytes_to_hex(&{
            use sha2::Digest;
            sha2::Sha256::digest(&data)
        });
        assert_eq!(sha256_hex, expect_sha);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_path_streams_100mib_with_sha256() {
        use std::time::Instant;
        let (mut host, mut joiner) = pair_host_joiner();
        let dir = std::env::temp_dir().join(format!(
            "m590-100m-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        host.set_file_receive_dir(dir.join("recv"));
        let src = dir.join("big-100m.bin");
        let n = 100 * 1024 * 1024;
        // Stream-write patterned file without holding two full copies longer than needed.
        {
            let mut f = File::create(&src).unwrap();
            let mut buf = vec![0u8; FILE_CHUNK_SIZE];
            let mut left = n;
            let mut seq = 0u8;
            while left > 0 {
                let take = left.min(buf.len());
                for b in &mut buf[..take] {
                    *b = seq;
                    seq = seq.wrapping_add(1);
                }
                f.write_all(&buf[..take]).unwrap();
                left -= take;
            }
        }
        let t0 = Instant::now();
        assert_eq!(
            joiner.offer_file_path("big-100", &src).unwrap(),
            QueueFileResult::Queued
        );
        let offer = joiner.take_outbox().pop().unwrap();
        host.handle(SessionEvent::Message(offer)).unwrap();
        let _ = host.take_inbound_file();
        host.request_file("big-100").unwrap();
        let req = host.take_outbox().pop().unwrap();
        joiner.handle(SessionEvent::Message(req)).unwrap();
        drain_file_send(&mut joiner, &mut host);
        let elapsed = t0.elapsed();
        let Some(InboundFileResult::Applied {
            path,
            size,
            sha256_hex,
            ..
        }) = host.take_inbound_file()
        else {
            panic!("expected 100MiB applied");
        };
        assert_eq!(size, n as u64);
        assert_eq!(fs::metadata(&path).unwrap().len(), n as u64);
        // Re-hash source for expected digest.
        let mut hasher = sha2::Sha256::new();
        let mut f = File::open(&src).unwrap();
        let mut buf = vec![0u8; FILE_CHUNK_SIZE];
        loop {
            let read = f.read(&mut buf).unwrap();
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        let expect = crate::bytes_to_hex(&hasher.finalize());
        assert_eq!(sha256_hex, expect);
        let mib = (n as f64) / (1024.0 * 1024.0);
        let secs = elapsed.as_secs_f64().max(1e-6);
        eprintln!(
            "100MiB stream: {mib:.1} MiB in {secs:.3}s => {:.1} MiB/s sha256={sha256_hex}",
            mib / secs
        );
        let _ = fs::remove_dir_all(&dir);
    }

}
