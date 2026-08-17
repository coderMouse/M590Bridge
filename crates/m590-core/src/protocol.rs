use std::collections::HashSet;

use crate::{DeviceId, ProtocolError};

/// Draft wire protocol version (frame header also carries this value).
pub const PROTOCOL_VERSION: u8 = 3;

/// Maximum decoded clipboard image area accepted by the application.
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;

/// Maximum inline clipboard image bytes accepted by the protocol.
pub const MAX_INLINE_IMAGE_BYTES: usize = 12 * 1024 * 1024;

/// Maximum file data carried by one `FileChunk` message.
pub const MAX_FILE_CHUNK_BYTES: usize = 256 * 1024;

const MAX_TRANSFER_ID_BYTES: usize = 128;
const MAX_FILE_STATUS_MESSAGE_BYTES: usize = 1024;

/// Maximum number of entries (files plus directories) in one batch manifest.
pub const MAX_BATCH_ENTRIES: usize = 4096;
/// Maximum number of slash-separated components in one batch relative path.
pub const MAX_BATCH_PATH_DEPTH: usize = 64;
/// Maximum encoded payload bytes for one batch manifest.
pub const MAX_BATCH_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
/// Maximum sum of file sizes announced by one batch manifest.
pub const MAX_BATCH_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Maximum display name bytes in a batch manifest.
pub const MAX_BATCH_DISPLAY_NAME_BYTES: usize = 255;
/// Maximum batch id bytes.
pub const MAX_BATCH_ID_BYTES: usize = 128;
/// Maximum relative path bytes in a batch entry.
pub const MAX_BATCH_PATH_BYTES: usize = 4096;
/// Maximum entry id bytes in a batch entry.
pub const MAX_BATCH_ENTRY_ID_BYTES: usize = 128;

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
        let pixels = (width as u64)
            .checked_mul(height as u64)
            .ok_or(ProtocolError::InvalidImage("dimensions overflow"))?;
        if pixels > MAX_IMAGE_PIXELS {
            return Err(ProtocolError::InvalidImage("dimensions exceed pixel limit"));
        }
        if data.len() > MAX_INLINE_IMAGE_BYTES {
            return Err(ProtocolError::InvalidImage(
                "image exceeds inline byte limit",
            ));
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

/// Validate the identifier used to name an in-progress file transfer.
///
/// The identifier is used as a single path component below the receive directory, so
/// accepting arbitrary UTF-8 or path syntax here would make the temporary-file boundary
/// depend on platform-specific path rules.
pub fn validate_transfer_id(transfer_id: &str) -> Result<(), ProtocolError> {
    if transfer_id.is_empty() {
        return Err(ProtocolError::EmptyTransferId);
    }
    if transfer_id.len() > MAX_TRANSFER_ID_BYTES
        || transfer_id == "."
        || transfer_id == ".."
        || !transfer_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ProtocolError::InvalidFile(
            "transfer id must be a safe single path component",
        ));
    }
    Ok(())
}

fn validate_safe_component(
    value: &str,
    max_bytes: usize,
    empty_error: ProtocolError,
    reason: &'static str,
) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Err(empty_error);
    }
    if value.len() > max_bytes
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ProtocolError::InvalidFile(reason));
    }
    Ok(())
}

/// Validate the stable id used to identify a batch manifest.
pub fn validate_batch_id(batch_id: &str) -> Result<(), ProtocolError> {
    validate_safe_component(
        batch_id,
        MAX_BATCH_ID_BYTES,
        ProtocolError::EmptyBatchId,
        "batch id must be a safe single path component",
    )
}

/// Validate the stable id used to identify one entry in a batch manifest.
pub fn validate_batch_entry_id(entry_id: &str) -> Result<(), ProtocolError> {
    validate_safe_component(
        entry_id,
        MAX_BATCH_ENTRY_ID_BYTES,
        ProtocolError::EmptyBatchEntryId,
        "batch entry id must be a safe single path component",
    )
}

/// Validate a slash-separated, platform-neutral relative path from a batch manifest.
///
/// Wire paths deliberately use `/` on both platforms. Backslashes, drive prefixes,
/// UNC prefixes, empty components, `.` and `..` are rejected instead of relying on
/// the host platform's `Path` parser.
pub fn validate_batch_relative_path(path: &str) -> Result<(), ProtocolError> {
    if path.is_empty() {
        return Err(ProtocolError::InvalidFile(
            "batch relative path must not be empty",
        ));
    }
    if path.len() > MAX_BATCH_PATH_BYTES || path.contains('\0') || path.contains('\\') {
        return Err(ProtocolError::InvalidFile(
            "batch relative path contains unsafe bytes",
        ));
    }
    if path.starts_with('/') {
        return Err(ProtocolError::InvalidFile(
            "batch relative path must not be absolute",
        ));
    }

    let mut components = path.split('/');
    let first = components.next().unwrap_or_default();
    if first.contains(':') {
        return Err(ProtocolError::InvalidFile(
            "batch relative path must not contain a Windows drive prefix",
        ));
    }

    let mut depth = 0usize;
    for component in std::iter::once(first).chain(components) {
        if component.is_empty() || component == "." || component == ".." {
            return Err(ProtocolError::InvalidFile(
                "batch relative path contains an unsafe component",
            ));
        }
        depth = depth.saturating_add(1);
    }
    if depth == 0 || depth > MAX_BATCH_PATH_DEPTH {
        return Err(ProtocolError::InvalidFile(
            "batch relative path exceeds depth limit",
        ));
    }
    Ok(())
}

/// Validate lowercase/uppercase hex SHA-256 (empty allowed = not provided).
pub fn validate_sha256_hex(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ProtocolError::InvalidFile(
            "sha256 must be 64 hex chars or empty",
        ));
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

/// Whether a batch entry is a regular file or a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BatchEntryKind {
    File = 0,
    Directory = 1,
}

impl BatchEntryKind {
    pub fn from_u8(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::File),
            1 => Ok(Self::Directory),
            _ => Err(ProtocolError::InvalidFile("unknown batch entry kind")),
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// One file or directory in a batch manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEntry {
    pub entry_id: String,
    /// Platform-neutral path using `/` separators, relative to the batch root.
    pub relative_path: String,
    pub kind: BatchEntryKind,
    /// Regular-file byte length; always zero for directories.
    pub size: u64,
    /// Optional lowercase hex SHA-256; always empty for directories.
    pub sha256_hex: String,
}

impl BatchEntry {
    pub fn new(
        entry_id: impl Into<String>,
        relative_path: impl Into<String>,
        kind: BatchEntryKind,
        size: u64,
        sha256_hex: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        let entry = Self {
            entry_id: entry_id.into(),
            relative_path: relative_path.into(),
            kind,
            size,
            sha256_hex: sha256_hex.into().to_ascii_lowercase(),
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn file(
        entry_id: impl Into<String>,
        relative_path: impl Into<String>,
        size: u64,
        sha256_hex: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        Self::new(
            entry_id,
            relative_path,
            BatchEntryKind::File,
            size,
            sha256_hex,
        )
    }

    pub fn directory(
        entry_id: impl Into<String>,
        relative_path: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        Self::new(entry_id, relative_path, BatchEntryKind::Directory, 0, "")
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_batch_entry_id(&self.entry_id)?;
        validate_batch_relative_path(&self.relative_path)?;
        match self.kind {
            BatchEntryKind::File => validate_sha256_hex(&self.sha256_hex),
            BatchEntryKind::Directory => {
                if self.size != 0 || !self.sha256_hex.is_empty() {
                    return Err(ProtocolError::InvalidFile(
                        "directory entry must not carry file size or sha256",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Peer announces a directory tree or a group of files for on-demand retrieval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileBatchOfferPayload {
    pub device_id: DeviceId,
    pub batch_id: String,
    pub display_name: String,
    pub entries: Vec<BatchEntry>,
}

impl FileBatchOfferPayload {
    pub fn new(
        device_id: DeviceId,
        batch_id: impl Into<String>,
        display_name: impl Into<String>,
        mut entries: Vec<BatchEntry>,
    ) -> Result<Self, ProtocolError> {
        for entry in &mut entries {
            entry.sha256_hex.make_ascii_lowercase();
        }
        let payload = Self {
            device_id,
            batch_id: batch_id.into(),
            display_name: display_name.into(),
            entries,
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Validate all identifiers, paths, limits and the encoded manifest size.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        validate_batch_id(&self.batch_id)?;
        if self.display_name.is_empty()
            || self.display_name.len() > MAX_BATCH_DISPLAY_NAME_BYTES
            || self.display_name.contains('\0')
        {
            return Err(ProtocolError::InvalidFile(
                "batch display name is empty or too long",
            ));
        }
        if self.entries.is_empty() {
            return Err(ProtocolError::InvalidFile(
                "batch manifest must contain at least one entry",
            ));
        }
        if self.entries.len() > MAX_BATCH_ENTRIES {
            return Err(ProtocolError::InvalidFile(
                "batch manifest contains too many entries",
            ));
        }

        let mut entry_ids = HashSet::with_capacity(self.entries.len());
        let mut paths = HashSet::with_capacity(self.entries.len());
        let mut total_bytes = 0u64;
        for entry in &self.entries {
            entry.validate()?;
            if !entry_ids.insert(&entry.entry_id) {
                return Err(ProtocolError::InvalidFile(
                    "batch manifest contains duplicate entry ids",
                ));
            }
            if !paths.insert(&entry.relative_path) {
                return Err(ProtocolError::InvalidFile(
                    "batch manifest contains duplicate relative paths",
                ));
            }
            if entry.kind == BatchEntryKind::File {
                total_bytes =
                    total_bytes
                        .checked_add(entry.size)
                        .ok_or(ProtocolError::InvalidFile(
                            "batch manifest total size overflow",
                        ))?;
                if total_bytes > MAX_BATCH_TOTAL_BYTES {
                    return Err(ProtocolError::InvalidFile(
                        "batch manifest total file size exceeds limit",
                    ));
                }
            }
        }
        if self.encoded_len()? > MAX_BATCH_MANIFEST_BYTES {
            return Err(ProtocolError::InvalidFile(
                "batch manifest exceeds encoded size limit",
            ));
        }
        Ok(())
    }

    /// Encoded payload length excluding the 12-byte frame header.
    pub fn encoded_len(&self) -> Result<usize, ProtocolError> {
        fn string_len(value: &str) -> Result<usize, ProtocolError> {
            if value.len() > u32::MAX as usize {
                return Err(ProtocolError::InvalidFile(
                    "batch manifest string is too long",
                ));
            }
            4usize
                .checked_add(value.len())
                .ok_or(ProtocolError::InvalidFile("batch manifest size overflow"))
        }

        let mut len = string_len(self.device_id.as_str())?;
        len = len
            .checked_add(string_len(&self.batch_id)?)
            .ok_or(ProtocolError::InvalidFile("batch manifest size overflow"))?;
        len = len
            .checked_add(string_len(&self.display_name)?)
            .and_then(|n| n.checked_add(4))
            .ok_or(ProtocolError::InvalidFile("batch manifest size overflow"))?;
        for entry in &self.entries {
            let mut entry_len = string_len(&entry.entry_id)?;
            entry_len = entry_len
                .checked_add(string_len(&entry.relative_path)?)
                .and_then(|n| n.checked_add(1 + 8))
                .ok_or(ProtocolError::InvalidFile("batch manifest size overflow"))?;
            entry_len = entry_len
                .checked_add(string_len(&entry.sha256_hex)?)
                .ok_or(ProtocolError::InvalidFile("batch manifest size overflow"))?;
            len = len
                .checked_add(entry_len)
                .ok_or(ProtocolError::InvalidFile("batch manifest size overflow"))?;
        }
        Ok(len)
    }
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
        validate_transfer_id(&transfer_id)?;
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
        validate_transfer_id(&transfer_id)?;
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
        validate_transfer_id(&transfer_id)?;
        if data.is_empty() {
            return Err(ProtocolError::InvalidFile("chunk data must not be empty"));
        }
        if data.len() > MAX_FILE_CHUNK_BYTES {
            return Err(ProtocolError::InvalidFile("chunk exceeds maximum size"));
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

/// Cancels a pending or active file transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCancelPayload {
    pub device_id: DeviceId,
    pub transfer_id: String,
    pub message: String,
}

impl FileCancelPayload {
    pub fn new(
        device_id: DeviceId,
        transfer_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        if device_id.as_str().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }
        let transfer_id = transfer_id.into();
        validate_transfer_id(&transfer_id)?;
        let message = message.into();
        if message.len() > MAX_FILE_STATUS_MESSAGE_BYTES {
            return Err(ProtocolError::InvalidFile(
                "file cancel message exceeds maximum size",
            ));
        }
        Ok(Self {
            device_id,
            transfer_id,
            message,
        })
    }
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
        validate_transfer_id(&transfer_id)?;
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
    PairReject { device_id: DeviceId, reason: String },
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
    /// Transfer was cancelled by either peer.
    FileCancel(FileCancelPayload),
    /// A directory tree or group of files is available for on-demand retrieval.
    FileBatchOffer(FileBatchOfferPayload),
    /// Graceful teardown.
    Goodbye { device_id: DeviceId, reason: String },
}

impl Message {
    pub fn hello(
        device_id: DeviceId,
        app_version: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
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

    pub fn file_cancel(payload: FileCancelPayload) -> Self {
        Self::FileCancel(payload)
    }

    pub fn file_batch_offer(payload: FileBatchOfferPayload) -> Self {
        Self::FileBatchOffer(payload)
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
            Self::FileCancel(_) => "file_cancel",
            Self::FileBatchOffer(_) => "file_batch_offer",
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
        let err =
            ClipboardImagePayload::new(DeviceId::new("a"), "c1", 1, 1, vec![0, 0, 0]).unwrap_err();
        assert_eq!(err, ProtocolError::InvalidImage("rgba length mismatch"));
    }

    #[test]
    fn accepts_tiny_image() {
        let img =
            ClipboardImagePayload::new(DeviceId::new("a"), "c1", 1, 1, vec![1, 2, 3, 255]).unwrap();
        assert_eq!(img.byte_len(), 4);
    }

    #[test]
    fn file_offer_rejects_path_separators() {
        let err = FileOfferPayload::new(DeviceId::new("a"), "t1", "a/b.txt", 1).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("file name must be a basename")
        );
    }

    #[test]
    fn file_offer_accepts_basename() {
        let offer = FileOfferPayload::new(DeviceId::new("a"), "t1", "note.txt", 12).unwrap();
        assert_eq!(offer.file_name, "note.txt");
        assert_eq!(offer.size, 12);
    }

    #[test]
    fn file_payload_rejects_unsafe_transfer_id() {
        let err =
            FileOfferPayload::new(DeviceId::new("a"), "../escape", "note.txt", 1).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("transfer id must be a safe single path component")
        );
        assert!(validate_transfer_id("safe-id_01.part").is_ok());
    }

    #[test]
    fn image_payload_rejects_excessive_pixel_area() {
        let err = ClipboardImagePayload::encoded(
            DeviceId::new("a"),
            "c1",
            MAX_IMAGE_PIXELS as u32 + 1,
            1,
            ImageEncoding::Png,
            vec![1],
        )
        .unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidImage("dimensions exceed pixel limit")
        );
    }

    #[test]
    fn file_chunk_rejects_excessive_chunk_size() {
        let err = FileChunkPayload::new(
            DeviceId::new("a"),
            "t1",
            0,
            vec![0; MAX_FILE_CHUNK_BYTES + 1],
        )
        .unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("chunk exceeds maximum size")
        );
    }

    #[test]
    fn file_cancel_rejects_excessive_message() {
        let err = FileCancelPayload::new(DeviceId::new("a"), "t1", "x".repeat(1025)).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("file cancel message exceeds maximum size")
        );
    }

    #[test]
    fn batch_manifest_accepts_nested_files_and_directories() {
        let entries = vec![
            BatchEntry::directory("dir-1", "photos").unwrap(),
            BatchEntry::file(
                "file-1",
                "photos/2026/image.png",
                42,
                "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899",
            )
            .unwrap(),
        ];
        let manifest =
            FileBatchOfferPayload::new(DeviceId::new("sender"), "batch-1", "photos", entries)
                .unwrap();
        assert_eq!(
            manifest.entries[1].sha256_hex,
            "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
        );
        assert!(manifest.encoded_len().unwrap() < MAX_BATCH_MANIFEST_BYTES);
    }

    #[test]
    fn batch_manifest_rejects_unsafe_paths() {
        for path in [
            "/tmp/file.txt",
            "../file.txt",
            "folder/../file.txt",
            "folder//file.txt",
            "folder/./file.txt",
            "C:/file.txt",
            "C:file.txt",
            "//server/share/file.txt",
            r"\\server\share\file.txt",
            "folder\\file.txt",
            "folder/\0file.txt",
        ] {
            assert!(
                validate_batch_relative_path(path).is_err(),
                "path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn batch_manifest_rejects_directory_file_metadata_and_limits() {
        let err = BatchEntry::new("dir-1", "folder", BatchEntryKind::Directory, 1, "").unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("directory entry must not carry file size or sha256")
        );

        let err = FileBatchOfferPayload::new(
            DeviceId::new("sender"),
            "batch-1",
            "files",
            vec![BatchEntry::file("file-1", "file.bin", MAX_BATCH_TOTAL_BYTES + 1, "").unwrap()],
        )
        .unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("batch manifest total file size exceeds limit")
        );
    }

    #[test]
    fn batch_manifest_enforces_depth_count_and_encoded_size_limits() {
        let deep_path = std::iter::repeat("d")
            .take(MAX_BATCH_PATH_DEPTH + 1)
            .collect::<Vec<_>>()
            .join("/");
        assert!(BatchEntry::file("deep-1", deep_path, 0, "").is_err());

        let too_many = (0..=MAX_BATCH_ENTRIES)
            .map(|index| BatchEntry::file(format!("entry-{index}"), format!("file-{index}"), 0, ""))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            FileBatchOfferPayload::new(DeviceId::new("sender"), "batch-1", "files", too_many)
                .is_err()
        );

        let prefix = "a".repeat(4000);
        let oversized = (0..1100)
            .map(|index| {
                BatchEntry::file(format!("entry-{index}"), format!("{prefix}/{index}"), 0, "")
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let err =
            FileBatchOfferPayload::new(DeviceId::new("sender"), "batch-1", "files", oversized)
                .unwrap_err();
        assert_eq!(
            err,
            ProtocolError::InvalidFile("batch manifest exceeds encoded size limit")
        );
    }
}
