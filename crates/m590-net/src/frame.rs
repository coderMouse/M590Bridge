//! Length-prefixed binary frame draft for M590Bridge messages.
//!
//! Layout:
//! - magic: 4 bytes `M590`
//! - version: u8
//! - msg_type: u8
//! - reserved: u16 BE (0)
//! - payload_len: u32 BE
//! - payload: type-specific fields (strings = u32 BE len + UTF-8 bytes; u64 = BE)

use m590_core::{
    ClipboardImagePayload, ClipboardTextPayload, DeviceId, FileChunkPayload, FileCompletePayload,
    FileOfferPayload, FileRequestPayload, ImageEncoding, Message, PROTOCOL_VERSION,
};

/// Wire magic bytes.
pub const FRAME_MAGIC: &[u8; 4] = b"M590";

/// Fixed header size in bytes.
pub const FRAME_HEADER_LEN: usize = 12;

/// Maximum payload size accepted by the draft decoder (16 MiB).
pub const MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

/// Frame encode/decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    BufferTooShort,
    InvalidMagic,
    UnsupportedVersion(u8),
    UnknownMessageType(u8),
    PayloadTooLarge(usize),
    TruncatedPayload,
    InvalidUtf8,
    InvalidField(&'static str),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooShort => write!(f, "buffer too short for frame header"),
            Self::InvalidMagic => write!(f, "invalid frame magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported protocol version {v}"),
            Self::UnknownMessageType(t) => write!(f, "unknown message type {t}"),
            Self::PayloadTooLarge(n) => write!(f, "payload too large: {n} bytes"),
            Self::TruncatedPayload => write!(f, "truncated payload"),
            Self::InvalidUtf8 => write!(f, "invalid utf-8 in payload"),
            Self::InvalidField(name) => write!(f, "invalid field: {name}"),
        }
    }
}

impl std::error::Error for FrameError {}

const TYPE_HELLO: u8 = 1;
const TYPE_HELLO_ACK: u8 = 2;
const TYPE_PAIR_REQUEST: u8 = 3;
const TYPE_PAIR_ACCEPT: u8 = 4;
const TYPE_PAIR_REJECT: u8 = 5;
const TYPE_HEARTBEAT: u8 = 6;
const TYPE_HEARTBEAT_ACK: u8 = 7;
const TYPE_CLIPBOARD_TEXT: u8 = 8;
const TYPE_GOODBYE: u8 = 9;
const TYPE_CLIPBOARD_IMAGE: u8 = 10;
const TYPE_FILE_OFFER: u8 = 11;
const TYPE_FILE_REQUEST: u8 = 12;
const TYPE_FILE_CHUNK: u8 = 13;
const TYPE_FILE_COMPLETE: u8 = 14;

/// Encode one message into a single frame buffer.
pub fn encode_frame(message: &Message) -> Result<Vec<u8>, FrameError> {
    let (msg_type, payload) = encode_payload(message)?;
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge(payload.len()));
    }

    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.extend_from_slice(FRAME_MAGIC);
    out.push(PROTOCOL_VERSION);
    out.push(msg_type);
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Decode exactly one frame from `data` (must be a complete frame, no streaming).
pub fn decode_frame(data: &[u8]) -> Result<Message, FrameError> {
    if data.len() < FRAME_HEADER_LEN {
        return Err(FrameError::BufferTooShort);
    }
    if &data[0..4] != FRAME_MAGIC {
        return Err(FrameError::InvalidMagic);
    }
    let version = data[4];
    if version != PROTOCOL_VERSION {
        return Err(FrameError::UnsupportedVersion(version));
    }
    let msg_type = data[5];
    let payload_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge(payload_len));
    }
    if data.len() < FRAME_HEADER_LEN + payload_len {
        return Err(FrameError::TruncatedPayload);
    }
    if data.len() != FRAME_HEADER_LEN + payload_len {
        // Draft decoder expects exact one-frame buffers.
        return Err(FrameError::InvalidField("trailing_bytes"));
    }
    decode_payload(msg_type, &data[FRAME_HEADER_LEN..])
}

fn encode_payload(message: &Message) -> Result<(u8, Vec<u8>), FrameError> {
    let mut payload = Vec::new();
    let msg_type = match message {
        Message::Hello {
            device_id,
            app_version,
        } => {
            write_string(&mut payload, device_id.as_str())?;
            write_string(&mut payload, app_version)?;
            TYPE_HELLO
        }
        Message::HelloAck {
            device_id,
            app_version,
        } => {
            write_string(&mut payload, device_id.as_str())?;
            write_string(&mut payload, app_version)?;
            TYPE_HELLO_ACK
        }
        Message::PairRequest {
            device_id,
            pairing_code,
        } => {
            write_string(&mut payload, device_id.as_str())?;
            write_string(&mut payload, pairing_code)?;
            TYPE_PAIR_REQUEST
        }
        Message::PairAccept { device_id } => {
            write_string(&mut payload, device_id.as_str())?;
            TYPE_PAIR_ACCEPT
        }
        Message::PairReject { device_id, reason } => {
            write_string(&mut payload, device_id.as_str())?;
            write_string(&mut payload, reason)?;
            TYPE_PAIR_REJECT
        }
        Message::Heartbeat { seq } => {
            payload.extend_from_slice(&seq.to_be_bytes());
            TYPE_HEARTBEAT
        }
        Message::HeartbeatAck { seq } => {
            payload.extend_from_slice(&seq.to_be_bytes());
            TYPE_HEARTBEAT_ACK
        }
        Message::ClipboardText(body) => {
            write_string(&mut payload, body.device_id.as_str())?;
            write_string(&mut payload, &body.content_id)?;
            write_string(&mut payload, &body.text)?;
            TYPE_CLIPBOARD_TEXT
        }
        Message::ClipboardImage(body) => {
            write_string(&mut payload, body.device_id.as_str())?;
            write_string(&mut payload, &body.content_id)?;
            payload.extend_from_slice(&body.width.to_be_bytes());
            payload.extend_from_slice(&body.height.to_be_bytes());
            payload.push(body.encoding.as_u8());
            write_bytes(&mut payload, &body.data)?;
            TYPE_CLIPBOARD_IMAGE
        }
        Message::FileOffer(body) => {
            write_string(&mut payload, body.device_id.as_str())?;
            write_string(&mut payload, &body.transfer_id)?;
            write_string(&mut payload, &body.file_name)?;
            payload.extend_from_slice(&body.size.to_be_bytes());
            TYPE_FILE_OFFER
        }
        Message::FileRequest(body) => {
            write_string(&mut payload, body.device_id.as_str())?;
            write_string(&mut payload, &body.transfer_id)?;
            TYPE_FILE_REQUEST
        }
        Message::FileChunk(body) => {
            write_string(&mut payload, body.device_id.as_str())?;
            write_string(&mut payload, &body.transfer_id)?;
            payload.extend_from_slice(&body.offset.to_be_bytes());
            write_bytes(&mut payload, &body.data)?;
            TYPE_FILE_CHUNK
        }
        Message::FileComplete(body) => {
            write_string(&mut payload, body.device_id.as_str())?;
            write_string(&mut payload, &body.transfer_id)?;
            payload.push(u8::from(body.ok));
            write_string(&mut payload, &body.message)?;
            TYPE_FILE_COMPLETE
        }
        Message::Goodbye { device_id, reason } => {
            write_string(&mut payload, device_id.as_str())?;
            write_string(&mut payload, reason)?;
            TYPE_GOODBYE
        }
    };
    Ok((msg_type, payload))
}

fn decode_payload(msg_type: u8, mut payload: &[u8]) -> Result<Message, FrameError> {
    match msg_type {
        TYPE_HELLO => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let app_version = read_string(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::Hello {
                device_id,
                app_version,
            })
        }
        TYPE_HELLO_ACK => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let app_version = read_string(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::HelloAck {
                device_id,
                app_version,
            })
        }
        TYPE_PAIR_REQUEST => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let pairing_code = read_string(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::PairRequest {
                device_id,
                pairing_code,
            })
        }
        TYPE_PAIR_ACCEPT => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            ensure_empty(payload)?;
            Ok(Message::PairAccept { device_id })
        }
        TYPE_PAIR_REJECT => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let reason = read_string(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::PairReject { device_id, reason })
        }
        TYPE_HEARTBEAT => {
            let seq = read_u64(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::Heartbeat { seq })
        }
        TYPE_HEARTBEAT_ACK => {
            let seq = read_u64(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::HeartbeatAck { seq })
        }
        TYPE_CLIPBOARD_TEXT => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let content_id = read_string(&mut payload)?;
            let text = read_string(&mut payload)?;
            ensure_empty(payload)?;
            let body = ClipboardTextPayload {
                device_id,
                content_id,
                text,
            };
            Ok(Message::ClipboardText(body))
        }
        TYPE_CLIPBOARD_IMAGE => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let content_id = read_string(&mut payload)?;
            let width = read_u32(&mut payload)?;
            let height = read_u32(&mut payload)?;
            if payload.is_empty() {
                return Err(FrameError::TruncatedPayload);
            }
            let encoding = ImageEncoding::from_u8(payload[0]).map_err(|e| {
                FrameError::InvalidField(match e {
                    m590_core::ProtocolError::InvalidImage(reason) => reason,
                    _ => "encoding",
                })
            })?;
            payload = &payload[1..];
            let data = read_bytes(&mut payload)?;
            ensure_empty(payload)?;
            let body = ClipboardImagePayload::encoded(
                device_id, content_id, width, height, encoding, data,
            )
            .map_err(|e| FrameError::InvalidField(match e {
                m590_core::ProtocolError::EmptyDeviceId => "device_id",
                m590_core::ProtocolError::EmptyContentId => "content_id",
                m590_core::ProtocolError::InvalidImage(reason) => reason,
                _ => "image",
            }))?;
            Ok(Message::ClipboardImage(body))
        }
        TYPE_FILE_OFFER => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let transfer_id = read_string(&mut payload)?;
            let file_name = read_string(&mut payload)?;
            let size = read_u64(&mut payload)?;
            ensure_empty(payload)?;
            let body = FileOfferPayload::new(device_id, transfer_id, file_name, size).map_err(
                |e| FrameError::InvalidField(match e {
                    m590_core::ProtocolError::EmptyDeviceId => "device_id",
                    m590_core::ProtocolError::EmptyTransferId => "transfer_id",
                    m590_core::ProtocolError::InvalidFile(reason) => reason,
                    _ => "file_offer",
                }),
            )?;
            Ok(Message::FileOffer(body))
        }
        TYPE_FILE_REQUEST => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let transfer_id = read_string(&mut payload)?;
            ensure_empty(payload)?;
            let body = FileRequestPayload::new(device_id, transfer_id).map_err(|e| {
                FrameError::InvalidField(match e {
                    m590_core::ProtocolError::EmptyDeviceId => "device_id",
                    m590_core::ProtocolError::EmptyTransferId => "transfer_id",
                    _ => "file_request",
                })
            })?;
            Ok(Message::FileRequest(body))
        }
        TYPE_FILE_CHUNK => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let transfer_id = read_string(&mut payload)?;
            let offset = read_u64(&mut payload)?;
            let data = read_bytes(&mut payload)?;
            ensure_empty(payload)?;
            let body = FileChunkPayload::new(device_id, transfer_id, offset, data).map_err(|e| {
                FrameError::InvalidField(match e {
                    m590_core::ProtocolError::EmptyDeviceId => "device_id",
                    m590_core::ProtocolError::EmptyTransferId => "transfer_id",
                    m590_core::ProtocolError::InvalidFile(reason) => reason,
                    _ => "file_chunk",
                })
            })?;
            Ok(Message::FileChunk(body))
        }
        TYPE_FILE_COMPLETE => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let transfer_id = read_string(&mut payload)?;
            if payload.is_empty() {
                return Err(FrameError::TruncatedPayload);
            }
            let ok = payload[0] != 0;
            payload = &payload[1..];
            let message = read_string(&mut payload)?;
            ensure_empty(payload)?;
            let body = FileCompletePayload::new(device_id, transfer_id, ok, message).map_err(
                |e| FrameError::InvalidField(match e {
                    m590_core::ProtocolError::EmptyDeviceId => "device_id",
                    m590_core::ProtocolError::EmptyTransferId => "transfer_id",
                    _ => "file_complete",
                }),
            )?;
            Ok(Message::FileComplete(body))
        }
        TYPE_GOODBYE => {
            let device_id = DeviceId::new(read_string(&mut payload)?);
            let reason = read_string(&mut payload)?;
            ensure_empty(payload)?;
            Ok(Message::Goodbye { device_id, reason })
        }
        other => Err(FrameError::UnknownMessageType(other)),
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), FrameError> {
    let len = value.len();
    if len > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge(len));
    }
    out.extend_from_slice(&(len as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_string(input: &mut &[u8]) -> Result<String, FrameError> {
    if input.len() < 4 {
        return Err(FrameError::TruncatedPayload);
    }
    let len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    *input = &input[4..];
    if input.len() < len {
        return Err(FrameError::TruncatedPayload);
    }
    let slice = &input[..len];
    *input = &input[len..];
    let s = std::str::from_utf8(slice).map_err(|_| FrameError::InvalidUtf8)?;
    Ok(s.to_string())
}

fn read_u64(input: &mut &[u8]) -> Result<u64, FrameError> {
    if input.len() < 8 {
        return Err(FrameError::TruncatedPayload);
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&input[..8]);
    *input = &input[8..];
    Ok(u64::from_be_bytes(buf))
}

fn write_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), FrameError> {
    let len = value.len();
    if len > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge(len));
    }
    out.extend_from_slice(&(len as u32).to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

fn read_bytes(input: &mut &[u8]) -> Result<Vec<u8>, FrameError> {
    if input.len() < 4 {
        return Err(FrameError::TruncatedPayload);
    }
    let len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    *input = &input[4..];
    if input.len() < len {
        return Err(FrameError::TruncatedPayload);
    }
    let slice = input[..len].to_vec();
    *input = &input[len..];
    Ok(slice)
}

fn read_u32(input: &mut &[u8]) -> Result<u32, FrameError> {
    if input.len() < 4 {
        return Err(FrameError::TruncatedPayload);
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&input[..4]);
    *input = &input[4..];
    Ok(u32::from_be_bytes(buf))
}


fn ensure_empty(input: &[u8]) -> Result<(), FrameError> {
    if input.is_empty() {
        Ok(())
    } else {
        Err(FrameError::InvalidField("trailing_payload_bytes"))
    }
}


/// Try to decode one complete frame from the front of `data`.
///
/// Returns `Ok(None)` if more bytes are needed. On success returns
/// `(message, bytes_consumed)`.
pub fn try_decode_frame(data: &[u8]) -> Result<Option<(Message, usize)>, FrameError> {
    if data.len() < FRAME_HEADER_LEN {
        return Ok(None);
    }
    if &data[0..4] != FRAME_MAGIC {
        return Err(FrameError::InvalidMagic);
    }
    let version = data[4];
    if version != PROTOCOL_VERSION {
        return Err(FrameError::UnsupportedVersion(version));
    }
    let payload_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(FrameError::PayloadTooLarge(payload_len));
    }
    let total = FRAME_HEADER_LEN + payload_len;
    if data.len() < total {
        return Ok(None);
    }
    let msg = decode_frame(&data[..total])?;
    Ok(Some((msg, total)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use m590_core::DeviceId;

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = encode_frame(&Message::heartbeat(1)).unwrap();
        bytes[0] = b'X';
        assert_eq!(decode_frame(&bytes).unwrap_err(), FrameError::InvalidMagic);
    }

    #[test]
    fn roundtrip_unicode_clipboard() {
        let msg = Message::clipboard_text(
            ClipboardTextPayload {
                device_id: DeviceId::new("dev"),
                content_id: "c".into(),
                text: "剪贴板 ✅".into(),
            },
        );
        let decoded = decode_frame(&encode_frame(&msg).unwrap()).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn roundtrip_clipboard_image() {
        let msg = Message::clipboard_image(
            ClipboardImagePayload::new(
                DeviceId::new("dev"),
                "img-1",
                2,
                1,
                vec![1, 2, 3, 255, 4, 5, 6, 255],
            )
            .unwrap(),
        );
        let decoded = decode_frame(&encode_frame(&msg).unwrap()).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn roundtrip_file_messages() {
        let samples = vec![
            Message::file_offer(
                FileOfferPayload::new(DeviceId::new("a"), "t1", "a.txt", 3).unwrap(),
            ),
            Message::file_request(FileRequestPayload::new(DeviceId::new("b"), "t1").unwrap()),
            Message::file_chunk(
                FileChunkPayload::new(DeviceId::new("a"), "t1", 0, b"abc".to_vec()).unwrap(),
            ),
            Message::file_complete(
                FileCompletePayload::new(DeviceId::new("a"), "t1", true, "").unwrap(),
            ),
            Message::file_complete(
                FileCompletePayload::new(DeviceId::new("a"), "t1", false, "nope").unwrap(),
            ),
        ];
        for msg in samples {
            let decoded = decode_frame(&encode_frame(&msg).unwrap()).unwrap();
            assert_eq!(decoded, msg);
        }
    }
}
