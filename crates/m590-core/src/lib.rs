//! Shared core types for M590Bridge: identity, protocol messages, session draft.

mod error;
mod protocol;
mod session;

pub use error::{ProtocolError, SessionError};
pub use protocol::{ClipboardTextPayload, Message, PROTOCOL_VERSION};
pub use session::{
    InboundClipboardResult, QueueClipboardResult, Session, SessionEvent, SessionSnapshot,
    DEFAULT_HEARTBEAT_MISS_THRESHOLD,
};

/// Crate / product version string used by the daemon binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application display name (product default; see open-questions Q1).
pub const APP_NAME: &str = "M590Bridge";

/// Device identifier reserved for multi-device-ready protocol design.
/// MVP runtime is 1 peer only; the field remains on wire messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// High-level connection / pairing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Pairing,
    Connected,
}

/// Clipboard pipeline state (no OS clipboard I/O in this crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    Idle,
    Syncing,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn device_id_roundtrip() {
        let id = DeviceId::new("dev-a");
        assert_eq!(id.as_str(), "dev-a");
        assert_eq!(id.to_string(), "dev-a");
    }
}
