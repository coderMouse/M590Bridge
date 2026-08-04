use crate::{DeviceId, ProtocolError};

/// Draft wire protocol version (frame header also carries this value).
pub const PROTOCOL_VERSION: u8 = 1;

/// Text clipboard payload carried on the wire after pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardTextPayload {
    /// Origin device (multi-device-ready; MVP has one peer).
    pub device_id: DeviceId,
    /// Caller-generated id used later to suppress echo loops.
    pub content_id: String,
    pub text: String,
}

impl ClipboardTextPayload {
    pub fn new(
        device_id: DeviceId,
        content_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let content_id = content_id.into();
        if content_id.is_empty() {
            return Err(ProtocolError::EmptyContentId);
        }
        Ok(Self {
            device_id,
            content_id,
            text: text.into(),
        })
    }
}

/// Application messages for pairing, heartbeat, and text clipboard sync.
///
/// File transfer messages are intentionally omitted (V2+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Initial hello from a device.
    Hello {
        device_id: DeviceId,
        app_version: String,
    },
    /// Hello acknowledgement from the peer.
    HelloAck {
        device_id: DeviceId,
        app_version: String,
    },
    /// Pairing request with a short code (manual IP + code flow).
    PairRequest {
        device_id: DeviceId,
        pairing_code: String,
    },
    /// Peer accepted pairing.
    PairAccept { device_id: DeviceId },
    /// Peer rejected pairing.
    PairReject {
        device_id: DeviceId,
        reason: String,
    },
    /// Liveness probe.
    Heartbeat { seq: u64 },
    /// Liveness response.
    HeartbeatAck { seq: u64 },
    /// MVP text clipboard sync.
    ClipboardText(ClipboardTextPayload),
    /// Graceful teardown.
    Goodbye {
        device_id: DeviceId,
        reason: String,
    },
}

impl Message {
    pub fn hello(device_id: DeviceId, app_version: impl Into<String>) -> Result<Self, ProtocolError> {
        validate_device(&device_id)?;
        Ok(Self::Hello {
            device_id,
            app_version: app_version.into(),
        })
    }

    pub fn hello_ack(
        device_id: DeviceId,
        app_version: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        validate_device(&device_id)?;
        Ok(Self::HelloAck {
            device_id,
            app_version: app_version.into(),
        })
    }

    pub fn pair_request(
        device_id: DeviceId,
        pairing_code: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        validate_device(&device_id)?;
        let pairing_code = pairing_code.into();
        if pairing_code.is_empty() {
            return Err(ProtocolError::EmptyPairingCode);
        }
        Ok(Self::PairRequest {
            device_id,
            pairing_code,
        })
    }

    pub fn pair_accept(device_id: DeviceId) -> Result<Self, ProtocolError> {
        validate_device(&device_id)?;
        Ok(Self::PairAccept { device_id })
    }

    pub fn pair_reject(
        device_id: DeviceId,
        reason: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        validate_device(&device_id)?;
        Ok(Self::PairReject {
            device_id,
            reason: reason.into(),
        })
    }

    pub fn heartbeat(seq: u64) -> Self {
        Self::Heartbeat { seq }
    }

    pub fn heartbeat_ack(seq: u64) -> Self {
        Self::HeartbeatAck { seq }
    }

    pub fn clipboard_text(payload: ClipboardTextPayload) -> Self {
        Self::ClipboardText(payload)
    }

    pub fn goodbye(device_id: DeviceId, reason: impl Into<String>) -> Result<Self, ProtocolError> {
        validate_device(&device_id)?;
        Ok(Self::Goodbye {
            device_id,
            reason: reason.into(),
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::HelloAck { .. } => "hello_ack",
            Self::PairRequest { .. } => "pair_request",
            Self::PairAccept { .. } => "pair_accept",
            Self::PairReject { .. } => "pair_reject",
            Self::Heartbeat { .. } => "heartbeat",
            Self::HeartbeatAck { .. } => "heartbeat_ack",
            Self::ClipboardText(_) => "clipboard_text",
            Self::Goodbye { .. } => "goodbye",
        }
    }
}

fn validate_device(device_id: &DeviceId) -> Result<(), ProtocolError> {
    if device_id.as_str().is_empty() {
        Err(ProtocolError::EmptyDeviceId)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_pairing_code() {
        let err = Message::pair_request(DeviceId::new("a"), "").unwrap_err();
        assert_eq!(err, ProtocolError::EmptyPairingCode);
    }

    #[test]
    fn clipboard_payload_requires_content_id() {
        let err = ClipboardTextPayload::new(DeviceId::new("a"), "", "hi").unwrap_err();
        assert_eq!(err, ProtocolError::EmptyContentId);
    }
}
