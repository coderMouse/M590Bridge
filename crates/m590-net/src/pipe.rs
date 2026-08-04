//! In-memory bidirectional pipe for protocol/session tests (no sockets).

use crate::frame::{decode_frame, encode_frame, FrameError};
use m590_core::{Message, Session, SessionError, SessionEvent};

/// Two FIFO byte queues representing A→B and B→A links.
#[derive(Debug, Default)]
pub struct MemoryPipe {
    a_to_b: Vec<Vec<u8>>,
    b_to_a: Vec<Vec<u8>>,
}

impl MemoryPipe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send_a_to_b(&mut self, message: &Message) -> Result<(), FrameError> {
        self.a_to_b.push(encode_frame(message)?);
        Ok(())
    }

    pub fn send_b_to_a(&mut self, message: &Message) -> Result<(), FrameError> {
        self.b_to_a.push(encode_frame(message)?);
        Ok(())
    }

    pub fn pending_a_to_b(&self) -> usize {
        self.a_to_b.len()
    }

    pub fn pending_b_to_a(&self) -> usize {
        self.b_to_a.len()
    }
}

/// Pump encoded frames between two sessions until both queues drain or steps exhaust.
pub fn deliver(
    pipe: &mut MemoryPipe,
    session_a: &mut Session,
    session_b: &mut Session,
) -> Result<(), DeliverError> {
    // First enqueue any currently pending logical outbox as frames.
    flush_outbox(session_a, pipe, true)?;
    flush_outbox(session_b, pipe, false)?;

    for _ in 0..64 {
        let mut progress = false;

        while let Some(frame) = pipe.a_to_b.first().cloned() {
            pipe.a_to_b.remove(0);
            let msg = decode_frame(&frame)?;
            session_b.handle(SessionEvent::Message(msg))?;
            flush_outbox(session_b, pipe, false)?;
            progress = true;
        }

        while let Some(frame) = pipe.b_to_a.first().cloned() {
            pipe.b_to_a.remove(0);
            let msg = decode_frame(&frame)?;
            session_a.handle(SessionEvent::Message(msg))?;
            flush_outbox(session_a, pipe, true)?;
            progress = true;
        }

        if !progress {
            break;
        }
    }

    if pipe.pending_a_to_b() > 0 || pipe.pending_b_to_a() > 0 {
        return Err(DeliverError::Stuck);
    }
    Ok(())
}

fn flush_outbox(
    session: &mut Session,
    pipe: &mut MemoryPipe,
    as_a: bool,
) -> Result<(), FrameError> {
    for msg in session.take_outbox() {
        if as_a {
            pipe.send_a_to_b(&msg)?;
        } else {
            pipe.send_b_to_a(&msg)?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum DeliverError {
    Frame(FrameError),
    Session(SessionError),
    Stuck,
}

impl std::fmt::Display for DeliverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frame(err) => write!(f, "frame error: {err}"),
            Self::Session(err) => write!(f, "session error: {err}"),
            Self::Stuck => write!(f, "memory pipe still has pending frames"),
        }
    }
}

impl std::error::Error for DeliverError {}

impl From<FrameError> for DeliverError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}

impl From<SessionError> for DeliverError {
    fn from(value: SessionError) -> Self {
        Self::Session(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m590_core::{ConnectionState, DeviceId, SessionEvent};

    #[test]
    fn memory_pipe_pairs_and_syncs_text() {
        let mut host = Session::new(DeviceId::new("host")).unwrap();
        host.handle(SessionEvent::StartPairing {
            expected_code: "654321".into(),
        })
        .unwrap();
        // Host holds the expected code; do not send its joiner-style outbox.
        let _ = host.take_outbox();

        let mut joiner = Session::new(DeviceId::new("joiner")).unwrap();
        joiner
            .handle(SessionEvent::StartPairing {
                expected_code: "654321".into(),
            })
            .unwrap();

        let mut pipe = MemoryPipe::new();
        // joiner is A, host is B
        flush_outbox(&mut joiner, &mut pipe, true).unwrap();
        deliver(&mut pipe, &mut joiner, &mut host).unwrap();

        assert_eq!(host.state(), ConnectionState::Connected);
        assert_eq!(joiner.state(), ConnectionState::Connected);

        joiner.queue_clipboard_text("n1", "sync-me").unwrap();
        flush_outbox(&mut joiner, &mut pipe, true).unwrap();
        deliver(&mut pipe, &mut joiner, &mut host).unwrap();

        assert_eq!(
            host.snapshot().last_clipboard_content_id.as_deref(),
            Some("n1")
        );
    }
}
