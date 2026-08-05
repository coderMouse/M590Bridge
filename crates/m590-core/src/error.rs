use std::fmt;

/// Protocol-level errors (encode/decode/validation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    EmptyDeviceId,
    EmptyPairingCode,
    EmptyContentId,
    EmptyTransferId,
    InvalidImage(&'static str),
    InvalidFile(&'static str),
    InvalidMessage(&'static str),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDeviceId => write!(f, "device id must not be empty"),
            Self::EmptyPairingCode => write!(f, "pairing code must not be empty"),
            Self::EmptyContentId => write!(f, "clipboard content id must not be empty"),
            Self::EmptyTransferId => write!(f, "file transfer id must not be empty"),
            Self::InvalidImage(reason) => write!(f, "invalid clipboard image: {reason}"),
            Self::InvalidFile(reason) => write!(f, "invalid file transfer: {reason}"),
            Self::InvalidMessage(reason) => write!(f, "invalid message: {reason}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Session state-machine errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    InvalidTransition {
        from: crate::ConnectionState,
        event: &'static str,
    },
    Protocol(ProtocolError),
    UnexpectedPeer(String),
    /// Peer rejected pairing (wrong/expired code, etc.).
    PairRejected(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, event } => {
                write!(f, "invalid transition from {from:?} on {event}")
            }
            Self::Protocol(err) => write!(f, "protocol error: {err}"),
            Self::UnexpectedPeer(id) => write!(f, "unexpected peer device id: {id}"),
            Self::PairRejected(reason) => {
                if reason.is_empty() {
                    write!(f, "pairing rejected by peer")
                } else {
                    write!(f, "pairing rejected by peer: {reason}")
                }
            }
        }
    }
}

impl std::error::Error for SessionError {}

impl From<ProtocolError> for SessionError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}
