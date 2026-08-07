use crate::{DeviceId, ProtocolError};

/// Draft wire protocol version (frame header also carries this value).
pub const PROTOCOL_VERSION: u8 = 2;

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

/// On-wire image encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImageEncoding {
    /// Row-major RGBA8, length must be width*height*4.
    RawRgba = 0,
    /// PNG bytes (preferred for large screenshots).
    Png = 1,
}

impl ImageEncoding {
    pub fn from_u8(v: u8) -> Result<Self, ProtocolError> {
        match v {
            0 => Ok(Self::RawRgba),
            1 => Ok(Self::Png),
            _ => Err(ProtocolError::InvalidImage("unknown image encoding")),
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Image clipboard payload carried inline on the wire.
///
/// Prefer [`ImageEncoding::Png`] for screenshots; raw RGBA kept for tiny images / tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImagePayload {
    pub device_id: DeviceId,
    pub content_id: String,
    pub width: u32,
    pub height: u32,
    pub encoding: ImageEncoding,
    /// Encoding-specific bytes (RGBA8 or PNG).
    pub data: Vec<u8>,
}

impl ClipboardImagePayload {
    /// Raw RGBA helper (tests / tiny images).
    pub fn new(
        device_id: DeviceId,
        content_id: impl Into<String>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Self, ProtocolError> {
        Self::encoded(
            device_id,
            content_id,
            width,
            height,
            ImageEncoding::RawRgba,
            rgba,
        )
    }

    pub fn encoded(
        device_id: DeviceId,
        content_id: impl Into<String>,
        width: u32,
        height: u32,
        encoding: ImageEncoding,
        data: Vec<u8>,
    ) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let content_id = content_id.into();
        if content_id.is_empty() {
            return Err(ProtocolError::EmptyContentId);
        }
        if width == 0 || height == 0 {
            return Err(ProtocolError::InvalidImage("dimensions must be non-zero"));
        }
        match encoding {
            ImageEncoding::RawRgba => {
                let expected = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or(ProtocolError::InvalidImage("dimensions overflow"))?;
                if data.len() != expected {
                    return Err(ProtocolError::InvalidImage("rgba length mismatch"));
                }
            }
            ImageEncoding::Png => {
                if data.is_empty() {
                    return Err(ProtocolError::InvalidImage("empty png data"));
                }
            }
        }
        Ok(Self {
            device_id,
            content_id,
            width,
            height,
            encoding,
            data,
        })
    }

    pub fn byte_len(&self) -> usize {
        self.data.len()
    }
}

fn validate_file_name(file_name: &str) -> Result<(), ProtocolError> {
    if file_name.is_empty() {
        return Err(ProtocolError::InvalidFile("file name must not be empty"));
    }
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains('\0') {
        return Err(ProtocolError::InvalidFile("file name must be a basename"));
    }
    if file_name == "." || file_name == ".." {
        return Err(ProtocolError::InvalidFile("file name must be a basename"));
    }
    Ok(())
}

/// Validate lowercase/uppercase hex SHA-256 (empty allowed = not provided).
pub fn validate_sha256_hex(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ProtocolError::InvalidFile("sha256 must be 64 hex chars or empty"));
    }
    Ok(())
}

/// Lowercase hex encode.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Peer announces a file is available for on-demand pull.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOfferPayload {
    pub device_id: DeviceId,
    pub transfer_id: String,
    pub file_name: String,
    pub size: u64,
    /// Lowercase hex SHA-256 of full file; empty if not precomputed.
    pub sha256_hex: String,
}

impl FileOfferPayload {
    pub fn new(
        device_id: DeviceId,
        transfer_id: impl Into<String>,
        file_name: impl Into<String>,
        size: u64,
    ) -> Result<Self, ProtocolError> {
        Self::with_sha256(device_id, transfer_id, file_name, size, "")
    }

    pub fn with_sha256(
        device_id: DeviceId,
        transfer_id: impl Into<String>,
        file_name: impl Into<String>,
        size: u64,
        sha256_hex: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId);
        }
        let file_name = file_name.into();
        validate_file_name(&file_name)?;
        let sha256_hex = sha256_hex.into().to_ascii_lowercase();
        validate_sha256_hex(&sha256_hex)?;
        Ok(Self {
            device_id,
            transfer_id,
            file_name,
            size,
            sha256_hex,
        })
    }
}

/// Receiver asks the offerer to start (or continue) sending bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRequestPayload {
    pub device_id: DeviceId,
    pub transfer_id: String,
}

impl FileRequestPayload {
    pub fn new(device_id: DeviceId, transfer_id: impl Into<String>) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId);
        }
        Ok(Self {
            device_id,
            transfer_id,
        })
    }
}

/// One slice of file bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChunkPayload {
    pub device_id: DeviceId,
    pub transfer_id: String,
    pub offset: u64,
    pub data: Vec<u8>,
}

impl FileChunkPayload {
    pub fn new(
        device_id: DeviceId,
        transfer_id: impl Into<String>,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId);
        }
        if data.is_empty() {
            return Err(ProtocolError::InvalidFile("chunk data must not be empty"));
        }
        Ok(Self {
            device_id,
            transfer_id,
            offset,
            data,
        })
    }
}

/// Transfer finished (success or failure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCompletePayload {
    pub device_id: DeviceId,
    pub transfer_id: String,
    pub ok: bool,
    pub message: String,
    /// Lowercase hex SHA-256 of transferred bytes when ok; empty otherwise.
    pub sha256_hex: String,
}

impl FileCompletePayload {
    pub fn new(
        device_id: DeviceId,
        transfer_id: impl Into<String>,
        ok: bool,
        message: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        Self::with_sha256(device_id, transfer_id, ok, message, "")
    }

    pub fn with_sha256(
        device_id: DeviceId,
        transfer_id: impl Into<String>,
        ok: bool,
        message: impl Into<String>,
        sha256_hex: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(ProtocolError::EmptyTransferId);
        }
        let sha256_hex = sha256_hex.into().to_ascii_lowercase();
        validate_sha256_hex(&sha256_hex)?;
        Ok(Self {
            device_id,
            transfer_id,
            ok,
            message: message.into(),
            sha256_hex,
        })
    }
}

/// Application messages for pairing, heartbeat, clipboard, and file transfer.
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
    /// V2 image clipboard sync (inline RGBA/PNG).
    ClipboardImage(ClipboardImagePayload),
    /// File available for on-demand pull.
    FileOffer(FileOfferPayload),
    /// Request bytes for a previously offered transfer.
    FileRequest(FileRequestPayload),
    /// File bytes slice.
    FileChunk(FileChunkPayload),
    /// Transfer finished.
    FileComplete(FileCompletePayload),
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

    pub fn clipboard_image(payload: ClipboardImagePayload) -> Self {
        Self::ClipboardImage(payload)
    }

    pub fn file_offer(payload: FileOfferPayload) -> Self {
        Self::FileOffer(payload)
    }

    pub fn file_request(payload: FileRequestPayload) -> Self {
        Self::FileRequest(payload)
    }

    pub fn file_chunk(payload: FileChunkPayload) -> Self {
        Self::FileChunk(payload)
    }

    pub fn file_complete(payload: FileCompletePayload) -> Self {
        Self::FileComplete(payload)
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
            Self::ClipboardImage(_) => "clipboard_image",
            Self::FileOffer(_) => "file_offer",
            Self::FileRequest(_) => "file_request",
            Self::FileChunk(_) => "file_chunk",
            Self::FileComplete(_) => "file_complete",
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

    #[test]
    fn rejects_bad_image_rgba_len() {
        let err = ClipboardImagePayload::new(DeviceId::new("a"), "c1", 1, 1, vec![0, 0, 0]).unwrap_err();
        assert_eq!(err, ProtocolError::InvalidImage("rgba length mismatch"));
    }

    #[test]
    fn accepts_tiny_image() {
        let img = ClipboardImagePayload::new(
            DeviceId::new("a"),
            "c1",
            1,
            1,
            vec![1, 2, 3, 255],
        )
        .unwrap();
        assert_eq!(img.byte_len(), 4);
    }

    #[test]
    fn file_offer_rejects_path_separators() {
        let err = FileOfferPayload::new(DeviceId::new("a"), "t1", "a/b.txt", 1).unwrap_err();
        assert_eq!(err, ProtocolError::InvalidFile("file name must be a basename"));
    }

    #[test]
    fn file_offer_accepts_basename() {
        let offer = FileOfferPayload::new(DeviceId::new("a"), "t1", "note.txt", 12).unwrap();
        assert_eq!(offer.file_name, "note.txt");
        assert_eq!(offer.size, 12);
    }
}
