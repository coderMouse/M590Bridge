use std::collections::VecDeque;

use crate::{
    ConnectionState, DeviceId, Message, ProtocolError, SessionError, SyncState, PROTOCOL_VERSION,
};

/// Max remembered clipboard content IDs for dedup (send + receive).
const SEEN_CONTENT_ID_CAP: usize = 64;

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
        rgba: Vec<u8>,
    },
    /// `content_id` already seen — ignored.
    DuplicateContentId,
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
    pub missed_heartbeat_acks: u32,
    pub outstanding_heartbeat_seq: Option<u64>,
}

/// In-memory 1-on-1 session. Types keep `DeviceId` for future multi-device use.
#[derive(Debug, Clone)]
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
    /// Outbound messages produced by the last `handle` / queue call.
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
            pending_outbox: Vec::new(),
        })
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

    pub fn handle(&mut self, event: SessionEvent) -> Result<(), SessionError> {
        self.pending_outbox.clear();
        self.last_inbound_clipboard = None;
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

    /// Queue outbound image after connected.
    pub fn queue_clipboard_image(
        &mut self,
        content_id: impl Into<String>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
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
        let fp = image_fingerprint(width, height, &rgba);
        if self.last_clipboard_image_fp == Some(fp) {
            return Ok(QueueClipboardResult::UnchangedImage);
        }
        if rgba.len() > Self::INLINE_IMAGE_MAX_BYTES {
            return Ok(QueueClipboardResult::ImageTooLarge {
                byte_len: rgba.len(),
                limit: Self::INLINE_IMAGE_MAX_BYTES,
            });
        }
        let payload = crate::ClipboardImagePayload::new(
            self.local_device.clone(),
            content_id,
            width,
            height,
            rgba,
        )?;
        self.sync_state = SyncState::Syncing;
        self.remember_content_id(payload.content_id.clone());
        self.last_clipboard_content_id = Some(payload.content_id.clone());
        self.last_clipboard_image_content_id = Some(payload.content_id.clone());
        self.last_clipboard_image_fp = Some(fp);
        self.last_clipboard_image_bytes = Some(payload.rgba.len());
        self.pending_outbox
            .push(Message::clipboard_image(payload));
        self.sync_state = SyncState::Idle;
        Ok(QueueClipboardResult::Queued)
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
            Message::PairReject { .. } => {
                self.reset_to_disconnected(true);
                Ok(())
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
        if payload.rgba.len() > Self::INLINE_IMAGE_MAX_BYTES {
            return Err(SessionError::Protocol(ProtocolError::InvalidImage(
                "image exceeds inline limit",
            )));
        }

        self.sync_state = SyncState::Syncing;
        self.remember_content_id(payload.content_id.clone());
        self.last_clipboard_content_id = Some(payload.content_id.clone());
        self.last_clipboard_image_content_id = Some(payload.content_id.clone());
        let fp = image_fingerprint(payload.width, payload.height, &payload.rgba);
        self.last_clipboard_image_fp = Some(fp);
        self.last_clipboard_image_bytes = Some(payload.rgba.len());
        self.last_inbound_clipboard = Some(InboundClipboardResult::AppliedImage {
            content_id: payload.content_id,
            width: payload.width,
            height: payload.height,
            rgba: payload.rgba,
        });
        self.sync_state = SyncState::Idle;
        Ok(())
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
        if clear_outbox {
            self.pending_outbox.clear();
        }
    }
}

fn image_fingerprint(width: u32, height: u32, rgba: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    rgba.hash(&mut hasher);
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
                rgba: rgba.clone(),
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
}
