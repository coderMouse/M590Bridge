//! Localhost HTTP control API for the operable UI shell.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use m590_clipboard::{ClipboardService, PlatformClipboard};
use m590_core::{
    BatchEntry, BatchEntryKind, BatchFileSource, ConnectionState, DeviceId, FileBatchOfferPayload,
    InboundClipboardResult, InboundFileResult, Message, QueueClipboardResult, QueueFileResult,
    Session, SessionEvent, DEFAULT_HEARTBEAT_MISS_THRESHOLD, MAX_BATCH_ENTRIES,
    MAX_BATCH_PATH_DEPTH, MAX_FILE_BYTES, MAX_MEMORY_FILE_BYTES,
};
use m590_net::{accept_framed, connect_framed_timeout, listen_on, TcpFrameStream};

use crate::config;
use crate::discovery::DiscoveryHandle;
use crate::file_save;
#[cfg(target_os = "linux")]
use crate::linux_virtual_file_manager::LinuxVirtualFileManager;
use crate::status::{persist_status_config, with_status, HubPhase, HubStatus, SharedStatus};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use crate::virtual_file_bridge::{BridgeEvent, PipeProducer, VirtualFileBridge};
#[cfg(target_os = "windows")]
use crate::windows_virtual_file_manager::{ManagerEvent, WindowsVirtualFileManager};

static STOP_BRIDGE: AtomicBool = AtomicBool::new(false);
static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static BRIDGE_STOPPING: AtomicBool = AtomicBool::new(false);
static BRIDGE_TRANSITION: Mutex<()> = Mutex::new(());
static NEXT_BATCH_ID: AtomicU64 = AtomicU64::new(1);

type SharedDiscovery = Arc<Mutex<Option<Arc<DiscoveryHandle>>>>;

fn discovery_handle(discovery: &SharedDiscovery) -> Option<Arc<DiscoveryHandle>> {
    discovery.lock().ok().and_then(|guard| guard.clone())
}

const HUB_TOKEN_ENV: &str = "M590_HUB_TOKEN";
const HUB_TOKEN_HEADER: &str = "x-m590-token";
const MAX_HTTP_HEADER_BYTES: usize = 64 * 1024;
const MAX_HTTP_JSON_BODY_BYTES: usize = 1024 * 1024;
pub const MAX_HTTP_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_HTTP_FILE_BODY_BYTES: usize = MAX_HTTP_FILE_BYTES.div_ceil(3) * 4 + 64 * 1024;
const IDLE_SESSION_LOOP_DELAY: Duration = Duration::from_millis(50);
const STALLED_FILE_LOOP_DELAY: Duration = Duration::from_millis(1);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);
const BRIDGE_STOP_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(target_os = "windows")]
const REPLACED_BATCH_REQUEST_GRACE: Duration = Duration::from_secs(2);

#[cfg(feature = "task-057-diagnostics")]
fn task_057_diagnostic(args: std::fmt::Arguments<'_>) {
    eprintln!("[task-057][hub] {args}");
}

#[cfg(not(feature = "task-057-diagnostics"))]
fn task_057_diagnostic(_args: std::fmt::Arguments<'_>) {}

#[cfg(target_os = "windows")]
struct WindowsVirtualReceive {
    transfer_id: String,
    file_name: String,
    size: u64,
    bridge: VirtualFileBridge,
    producer: PipeProducer,
    requested: bool,
    completed: bool,
    clipboard_replaced: bool,
    published_at: Instant,
    requested_at: Option<Instant>,
    network_started_at: Option<Instant>,
    first_chunk_at: Option<Instant>,
}

#[cfg(target_os = "windows")]
struct WindowsVirtualBatchFile {
    descriptor_index: usize,
    entry: BatchEntry,
    bridge: VirtualFileBridge,
    producer: PipeProducer,
    requested: bool,
    completed: bool,
    requested_at: Option<Instant>,
    network_started_at: Option<Instant>,
    first_chunk_at: Option<Instant>,
}

#[cfg(target_os = "windows")]
struct WindowsVirtualBatchReceive {
    batch_id: String,
    files: Vec<WindowsVirtualBatchFile>,
    published_at: Instant,
    active_index: Option<usize>,
    completed_files: u32,
    completed_bytes: u64,
    clipboard_replaced: bool,
    clipboard_replaced_idle_since: Option<Instant>,
}

#[cfg(target_os = "windows")]
impl WindowsVirtualBatchReceive {
    fn file_index(&self, transfer_id: &str) -> Option<usize> {
        self.files
            .iter()
            .position(|file| file.entry.entry_id == transfer_id)
    }

    fn must_finish(&self) -> bool {
        self.files
            .iter()
            .any(|file| file.requested && !file.completed)
    }

    fn is_complete(&self) -> bool {
        self.files.iter().all(|file| file.completed)
    }

    fn pending_ids(&self) -> impl Iterator<Item = String> + '_ {
        self.files
            .iter()
            .filter(|file| !file.completed)
            .map(|file| file.entry.entry_id.clone())
    }

    fn next_requested_index(&self) -> Option<usize> {
        self.files
            .iter()
            .position(|file| file.requested && !file.completed)
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsVirtualBatchReceive {
    fn drop(&mut self) {
        for file in &self.files {
            if !file.completed {
                file.producer.fail("virtual batch receiver stopped");
            }
        }
    }
}

#[cfg(target_os = "linux")]
struct LinuxVirtualReceive {
    transfer_id: String,
    bridge: VirtualFileBridge,
    producer: PipeProducer,
    requested: bool,
    completed: bool,
    consumed: bool,
    released: bool,
    clipboard_replaced: bool,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct PendingLinuxVirtualChunk {
    transfer_id: String,
    data: Vec<u8>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxVirtualChunkPush {
    Accepted,
    Backpressured,
    NoReceiver,
}

#[cfg(target_os = "linux")]
enum LinuxVirtualBatchStreamEvent {
    Unhandled,
    Handled(Option<PendingLinuxVirtualChunk>),
}

#[cfg(target_os = "linux")]
struct LinuxVirtualBatchFile {
    entry: BatchEntry,
    bridge: VirtualFileBridge,
    producer: PipeProducer,
    requested: bool,
    completed: bool,
    consumed: bool,
    released: bool,
}

#[cfg(target_os = "linux")]
struct LinuxVirtualBatchReceive {
    batch_id: String,
    files: Vec<LinuxVirtualBatchFile>,
    active_index: Option<usize>,
    completed_files: u32,
    completed_bytes: u64,
    clipboard_replaced: bool,
}

#[cfg(target_os = "linux")]
impl LinuxVirtualBatchReceive {
    fn file_index(&self, transfer_id: &str) -> Option<usize> {
        self.files
            .iter()
            .position(|file| file.entry.entry_id == transfer_id)
    }

    fn must_finish(&self) -> bool {
        self.files.iter().any(|file| {
            linux_virtual_receive_must_finish(
                file.requested,
                file.completed,
                file.consumed,
                file.released,
            )
        })
    }

    fn is_complete(&self) -> bool {
        self.files.iter().all(|file| file.completed)
    }

    fn pending_ids(&self) -> impl Iterator<Item = String> + '_ {
        self.files
            .iter()
            .filter(|file| !file.completed)
            .map(|file| file.entry.entry_id.clone())
    }

    fn next_requested_index(&self) -> Option<usize> {
        self.files
            .iter()
            .position(|file| file.requested && !file.completed)
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxVirtualBatchReceive {
    fn drop(&mut self) {
        for file in &self.files {
            if !file.completed {
                file.producer.fail("virtual batch receiver stopped");
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[derive(Debug)]
struct DeferredVirtualOffer {
    transfer_id: String,
    file_name: String,
    size: u64,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[derive(Debug)]
struct DeferredVirtualBatchOffer {
    batch_id: String,
    display_name: String,
    entries: Vec<BatchEntry>,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl DeferredVirtualBatchOffer {
    fn file_ids(&self) -> impl Iterator<Item = String> + '_ {
        self.entries
            .iter()
            .filter(|entry| entry.kind == BatchEntryKind::File)
            .map(|entry| entry.entry_id.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedFileStatus {
    summary: String,
    transfer_id: String,
    file_name: String,
    bytes: u64,
}

#[derive(Debug)]
struct PreparedBatch {
    batch_id: String,
    display_name: String,
    entries: Vec<BatchEntry>,
    sources: Vec<BatchFileSource>,
}

#[derive(Debug)]
struct OutboundBatchState {
    batch_id: String,
    display_name: String,
    files: Vec<BatchEntry>,
    pending_ids: HashSet<String>,
    completed_files: u32,
    completed_bytes: u64,
    total_bytes: u64,
}

impl OutboundBatchState {
    fn from_prepared(prepared: &PreparedBatch) -> Self {
        let files: Vec<BatchEntry> = prepared
            .entries
            .iter()
            .filter(|entry| entry.kind == BatchEntryKind::File)
            .cloned()
            .collect();
        Self {
            batch_id: prepared.batch_id.clone(),
            display_name: prepared.display_name.clone(),
            pending_ids: files.iter().map(|entry| entry.entry_id.clone()).collect(),
            completed_files: 0,
            completed_bytes: 0,
            total_bytes: files.iter().map(|entry| entry.size).sum(),
            files,
        }
    }

    fn entry(&self, entry_id: &str) -> Option<&BatchEntry> {
        self.files.iter().find(|entry| entry.entry_id == entry_id)
    }
}

#[derive(Debug)]
struct InboundBatchState {
    batch_id: String,
    display_name: String,
    entries: Vec<BatchEntry>,
    files: Vec<BatchEntry>,
    pending_ids: HashSet<String>,
    current_index: usize,
    completed_files: u32,
    completed_bytes: u64,
    total_bytes: u64,
    save_dir: PathBuf,
    partial_dir: PathBuf,
    staging_dir: PathBuf,
    committed: bool,
}

impl InboundBatchState {
    fn current(&self) -> Option<&BatchEntry> {
        self.files.get(self.current_index)
    }
}

impl Drop for InboundBatchState {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for entry in &self.files {
            let _ = fs::remove_file(self.partial_dir.join(format!("{}.part", entry.entry_id)));
        }
        let _ = fs::remove_dir_all(&self.staging_dir);
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn active_virtual_receive_must_finish(requested: bool, completed: bool) -> bool {
    requested && !completed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipboardReplacementDisposition {
    KeepActiveTransfer,
    ReleaseClipboardOffer,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn clipboard_replacement_disposition(
    requested: bool,
    completed: bool,
) -> ClipboardReplacementDisposition {
    if active_virtual_receive_must_finish(requested, completed) {
        ClipboardReplacementDisposition::KeepActiveTransfer
    } else {
        ClipboardReplacementDisposition::ReleaseClipboardOffer
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn completed_replaced_virtual_receive_can_detach(
    completed: bool,
    clipboard_replaced: bool,
) -> bool {
    completed && clipboard_replaced
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_virtual_receive_must_finish(
    requested: bool,
    completed: bool,
    consumed: bool,
    released: bool,
) -> bool {
    requested && !(completed && consumed && released)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_completed_replaced_virtual_receive_can_detach(
    completed: bool,
    consumed: bool,
    released: bool,
    clipboard_replaced: bool,
) -> bool {
    completed && consumed && released && clipboard_replaced
}

#[derive(Debug, PartialEq, Eq)]
enum SessionLoopPause {
    Yield,
    Sleep(Duration),
}

fn session_loop_pause(file_active: bool, file_progressed: bool) -> SessionLoopPause {
    if file_progressed {
        SessionLoopPause::Yield
    } else if file_active {
        SessionLoopPause::Sleep(STALLED_FILE_LOOP_DELAY)
    } else {
        SessionLoopPause::Sleep(IDLE_SESSION_LOOP_DELAY)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    path: String,
    body: String,
    origin: Option<String>,
    auth_token: Option<String>,
}

pub fn generate_hub_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("generate hub token: {e}"))?;
    Ok(m590_core::bytes_to_hex(&bytes))
}

pub fn run_hub(api_addr: &str) -> Result<(), String> {
    let auth_token = match std::env::var(HUB_TOKEN_ENV) {
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => {
            let generated = generate_hub_token()?;
            println!("hub_auth_token={generated}");
            generated
        }
    };
    run_hub_with_token(api_addr, auth_token)
}

pub fn run_hub_with_token(api_addr: &str, auth_token: String) -> Result<(), String> {
    run_hub_with_token_on_ready(api_addr, auth_token, None)
}

/// Same as [`run_hub_with_token`], but invokes `on_ready` after the control API socket is bound
/// and before the accept loop blocks. mDNS starts in a background thread so a slow/hung
/// multicast stack cannot keep `/api/health` offline.
pub fn run_hub_with_token_on_ready(
    api_addr: &str,
    auth_token: String,
    on_ready: Option<Box<dyn FnOnce() + Send>>,
) -> Result<(), String> {
    if auth_token.trim().len() < 32 {
        return Err("hub auth token must be at least 32 characters".into());
    }
    let auth_token: Arc<str> = Arc::from(auth_token);
    clear_pending_commands();
    let shared = crate::status::new_shared_status();
    with_status(&shared, |s| {
        s.hub_api = Some(format!("http://{api_addr}"));
        s.phase = HubPhase::Idle;
        s.file_clipboard_watch_likely = m590_clipboard::file_clipboard_watch_likely();
    });
    if !m590_clipboard::file_clipboard_watch_likely() {
        println!(
            "file_clipboard_watch=limited (Wayland without data-control; use UI pick/drag to send files)"
        );
    }

    // Bind the localhost control API first so the desktop shell can become online even when
    // mDNS startup is slow or blocked at login.
    let listener = TcpListener::bind(api_addr).map_err(|e| format!("bind hub api failed: {e}"))?;
    println!("hub_api=http://{api_addr}");
    println!("hub_auth=required header=X-M590-Token");
    println!("hub_status=ready (UI can open operable shell and point API to this address)");
    println!(
        "endpoints=GET /api/status /api/config /api/discover POST /api/discover/refresh /api/listen /api/connect /api/push /api/send_file /api/send_batch /api/cancel_batch /api/disconnect /api/config"
    );
    let cfg_path = config::default_config_path();
    println!("config_path={}", cfg_path.display());
    if let Some(cb) = on_ready {
        cb();
    }

    let device_id = with_status(&shared, |s| s.device_id.clone());
    let discovery: SharedDiscovery = Arc::new(Mutex::new(None));
    let discovery_for_mdns = Arc::clone(&discovery);
    thread::Builder::new()
        .name("m590-mdns-init".into())
        .spawn(move || match DiscoveryHandle::start(device_id) {
            Ok(h) => {
                if let Ok(mut slot) = discovery_for_mdns.lock() {
                    *slot = Some(h);
                }
                println!("mdns_browse=on type={}", crate::discovery::SERVICE_TYPE);
            }
            Err(err) => {
                eprintln!("mdns_browse=off error={err}");
            }
        })
        .map_err(|e| format!("spawn mdns init: {e}"))?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let shared = Arc::clone(&shared);
                let discovery = Arc::clone(&discovery);
                let auth_token = Arc::clone(&auth_token);
                thread::spawn(move || {
                    if let Err(err) = handle_http(stream, shared, discovery, auth_token) {
                        eprintln!("hub_http_error={err}");
                    }
                });
            }
            Err(err) => eprintln!("hub_accept_error={err}"),
        }
    }
    Ok(())
}

fn handle_http(
    mut stream: TcpStream,
    shared: SharedStatus,
    discovery: SharedDiscovery,
    auth_token: Arc<str>,
) -> Result<(), String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let request = read_http_request(&mut stream)?;
    let cors_origin = request
        .origin
        .as_deref()
        .filter(|origin| origin_allowed(origin));

    if request.origin.is_some() && cors_origin.is_none() {
        return write_json_error(&mut stream, 403, "origin not allowed", None);
    }

    if request.method == "OPTIONS" {
        return write_response(&mut stream, 204, "text/plain", "", cors_origin);
    }

    if !token_matches(&auth_token, request.auth_token.as_deref()) {
        return write_json_error(&mut stream, 401, "hub authentication required", cors_origin);
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/health") => write_response(
            &mut stream,
            200,
            "application/json",
            "{\"ok\":true}",
            cors_origin,
        ),
        ("GET", "/api/status") => {
            let json = with_status(&shared, |s| s.to_json());
            write_response(&mut stream, 200, "application/json", &json, cors_origin)
        }
        ("GET", "/api/config") => {
            let json = with_status(&shared, |s| s.snapshot_config().to_json());
            write_response(&mut stream, 200, "application/json", &json, cors_origin)
        }
        ("GET", "/api/discover") => {
            let json = match discovery_handle(&discovery).as_ref() {
                Some(d) => d.to_json(),
                None => {
                    "{\"service_type\":\"_m590bridge._tcp.local.\",\"advertising\":false,\"peers\":[],\"error\":\"mdns unavailable\"}".into()
                }
            };
            write_response(&mut stream, 200, "application/json", &json, cors_origin)
        }
        ("POST", "/api/discover/refresh") => match discovery_handle(&discovery).as_ref() {
            Some(d) => match d.refresh() {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    &d.to_json(),
                    cors_origin,
                ),
                Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
            },
            None => write_json_error(&mut stream, 400, "mdns unavailable", cors_origin),
        },
        ("POST", "/api/config") => match apply_config_update(&shared, &request.body) {
            Ok(json) => write_response(&mut stream, 200, "application/json", &json, cors_origin),
            Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
        },
        ("POST", "/api/listen") => {
            let code = resolve_pairing_code(&shared, &request.body);
            let port = json_get(&request.body, "port")
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(|| with_status(&shared, |s| s.listen_port));
            let device_id = json_get(&request.body, "device_id").filter(|s| !s.is_empty());
            match start_listen(shared, code, port, device_id, discovery) {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    "{\"ok\":true}",
                    cors_origin,
                ),
                Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
            }
        }
        ("POST", "/api/connect") => {
            let code = resolve_pairing_code(&shared, &request.body);
            let addr = json_get(&request.body, "addr")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    with_status(&shared, |s| s.connect_addr.clone().unwrap_or_default())
                });
            let device_id = json_get(&request.body, "device_id").filter(|s| !s.is_empty());
            match start_connect(shared, code, addr, device_id, discovery) {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    "{\"ok\":true}",
                    cors_origin,
                ),
                Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
            }
        }
        ("POST", "/api/push") => {
            let text = json_get(&request.body, "text").unwrap_or_default();
            match push_text(&shared, text) {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    "{\"ok\":true}",
                    cors_origin,
                ),
                Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
            }
        }
        ("POST", "/api/send_file") => {
            let file_path = json_get(&request.body, "path").unwrap_or_default();
            match push_file(&shared, file_path) {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    "{\"ok\":true}",
                    cors_origin,
                ),
                Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
            }
        }
        ("POST", "/api/send_batch") => match json_string_array(&request.body, "paths")
            .and_then(|paths| push_batch(&shared, paths))
        {
            Ok(()) => write_response(
                &mut stream,
                200,
                "application/json",
                "{\"ok\":true}",
                cors_origin,
            ),
            Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
        },
        ("POST", "/api/cancel_batch") => match queue_batch_cancel(&shared) {
            Ok(()) => write_response(
                &mut stream,
                200,
                "application/json",
                "{\"ok\":true}",
                cors_origin,
            ),
            Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
        },
        ("POST", "/api/send_file_bytes") => {
            let name = json_get(&request.body, "name").unwrap_or_default();
            let data_b64 = json_get(&request.body, "data_base64").unwrap_or_default();
            match push_file_bytes(&shared, name, data_b64) {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    "{\"ok\":true}",
                    cors_origin,
                ),
                Err(err) => write_json_error(&mut stream, 400, &err, cors_origin),
            }
        }
        ("POST", "/api/disconnect") => match stop_bridge(&shared, &discovery) {
            Ok(()) => write_response(
                &mut stream,
                200,
                "application/json",
                "{\"ok\":true}",
                cors_origin,
            ),
            Err(err) => write_json_error(&mut stream, 500, &err, cors_origin),
        },
        _ => write_response(
            &mut stream,
            404,
            "application/json",
            "{\"error\":\"not found\"}",
            cors_origin,
        ),
    }
}

fn apply_config_update(shared: &SharedStatus, body: &str) -> Result<String, String> {
    let mut cfg = with_status(shared, |s| s.snapshot_config());
    cfg.apply_json_patch(body);
    with_status(shared, |s| {
        s.apply_config(&cfg);
        // Live toggle for clipboard auto sync without restart.
        s.auto_sync = cfg.auto_sync;
        s.auto_reconnect = cfg.auto_reconnect;
    });
    config::save_config(&cfg)?;
    Ok(cfg.to_json())
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut data: Vec<u8> = Vec::with_capacity(4096);
    let mut buf = [0u8; 2048];
    loop {
        if find_header_end(&data).is_some() {
            break;
        }
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.len() > MAX_HTTP_HEADER_BYTES {
            return Err("http headers too large".into());
        }
    }
    if data.is_empty() {
        return Err("empty request".into());
    }
    let header_end = match find_header_end(&data) {
        Some(p) => p,
        None => return Err("incomplete http headers".into()),
    };

    let header_bytes = &data[..header_end];
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = None;
    let mut origin = None;
    let mut auth_token = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "invalid content-length")?,
                );
            }
            "origin" => origin = Some(value.trim().to_string()),
            HUB_TOKEN_HEADER => auth_token = Some(value.trim().to_string()),
            _ => {}
        }
    }

    let content_length = content_length.unwrap_or(0);
    let body_limit = http_body_limit(&path);
    if content_length > body_limit {
        return Err(format!(
            "http body too large: {content_length}B > limit {body_limit}B"
        ));
    }
    let mut body = data[header_end + 4..].to_vec();
    if body.len() > body_limit {
        return Err(format!(
            "http body too large: {}B > limit {body_limit}B",
            body.len()
        ));
    }
    while body.len() < content_length {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
        if body.len() > body_limit {
            return Err(format!(
                "http body too large: {}B > limit {body_limit}B",
                body.len()
            ));
        }
    }
    if body.len() < content_length {
        return Err("incomplete http body".into());
    }
    if content_length > 0 {
        body.truncate(content_length);
    }
    let body = String::from_utf8_lossy(&body)
        .trim_end_matches('\0')
        .to_string();
    Ok(HttpRequest {
        method,
        path,
        body,
        origin,
        auth_token,
    })
}

fn http_body_limit(path: &str) -> usize {
    if path == "/api/send_file_bytes" {
        MAX_HTTP_FILE_BODY_BYTES
    } else {
        MAX_HTTP_JSON_BODY_BYTES
    }
}

fn token_matches(expected: &str, provided: Option<&str>) -> bool {
    let Some(provided) = provided else {
        return false;
    };
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .as_bytes()
        .iter()
        .zip(provided.as_bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn origin_allowed(origin: &str) -> bool {
    if matches!(
        origin,
        "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
    ) {
        return true;
    }
    cfg!(debug_assertions) && is_local_dev_origin(origin)
}

fn is_local_dev_origin(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let Some((host, port)) = authority.rsplit_once(':') else {
        return false;
    };
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") && port.parse::<u16>().is_ok()
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w == b"\r\n\r\n")
}

fn normalize_pairing_code(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).take(6).collect()
}

fn resolve_pairing_code(shared: &SharedStatus, body: &str) -> String {
    let from_body = json_get(body, "code")
        .map(|s| normalize_pairing_code(&s))
        .filter(|s| !s.is_empty());
    if let Some(code) = from_body {
        return code;
    }
    with_status(shared, |s| {
        s.pairing_code
            .as_ref()
            .map(|c| normalize_pairing_code(c))
            .filter(|c| !c.is_empty())
            .unwrap_or_default()
    })
}

fn write_json_error(
    stream: &mut TcpStream,
    status: u16,
    err: &str,
    cors_origin: Option<&str>,
) -> Result<(), String> {
    let body = format!(
        "{{\"ok\":false,\"error\":\"{}\"}}",
        err.replace('\\', "\\\\").replace('"', "\\\"")
    );
    write_response(stream, status, "application/json", &body, cors_origin)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
    cors_origin: Option<&str>,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let cors_headers = cors_origin
        .map(|origin| {
            format!(
                "Access-Control-Allow-Origin: {origin}\r\n\
Vary: Origin\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, X-M590-Token\r\n"
            )
        })
        .unwrap_or_default();
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\n\
Content-Type: {content_type}; charset=utf-8\r\n\
Content-Length: {len}\r\n\
{cors_headers}\
Connection: close\r\n\
\r\n\
{body}",
        len = body.len(),
    );
    stream
        .write_all(resp.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn json_get(body: &str, key: &str) -> Option<String> {
    let object: serde_json::Value = serde_json::from_str(body).ok()?;
    match object.get(key)? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn json_string_array(body: &str, key: &str) -> Result<Vec<String>, String> {
    let object: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("invalid JSON: {err}"))?;
    let values = object
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{key} must be an array of strings"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must contain only strings"))
        })
        .collect()
}

fn claim_bridge() -> Result<(), String> {
    if BRIDGE_STOPPING.load(Ordering::SeqCst) {
        return Err("bridge is still stopping; try again shortly".into());
    }
    if BRIDGE_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("bridge already running; disconnect first".into());
    }
    STOP_BRIDGE.store(false, Ordering::SeqCst);
    Ok(())
}

fn stop_bridge(shared: &SharedStatus, discovery: &SharedDiscovery) -> Result<(), String> {
    let _transition = BRIDGE_TRANSITION
        .lock()
        .map_err(|_| "bridge lifecycle lock poisoned".to_string())?;

    if BRIDGE_RUNNING.load(Ordering::SeqCst) {
        BRIDGE_STOPPING.store(true, Ordering::SeqCst);
        STOP_BRIDGE.store(true, Ordering::SeqCst);
    }
    if !wait_until_stopped(&BRIDGE_RUNNING, &BRIDGE_STOPPING, BRIDGE_STOP_TIMEOUT) {
        return Err(format!(
            "bridge stop timeout ({}s): worker is still shutting down",
            BRIDGE_STOP_TIMEOUT.as_secs()
        ));
    }

    {
        let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
        with_status(shared, |s| {
            s.phase = HubPhase::Idle;
            s.connection = Some(ConnectionState::Disconnected);
            s.peer_device = None;
            s.last_error = None;
            s.role = None;
            s.endpoint = None;
            // Keep pairing_code / connect_addr / listen_port for UI prefills.
            s.reconnect_attempt = 0;
        });
        pending.clear();
    }
    if let Some(d) = discovery_handle(discovery).as_ref() {
        d.stop_advertise();
    }
    Ok(())
}

fn wait_until_stopped(running: &AtomicBool, stopping: &AtomicBool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while running.load(Ordering::SeqCst) || stopping.load(Ordering::SeqCst) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
    true
}

fn start_listen(
    shared: SharedStatus,
    mut code: String,
    port: u16,
    device_id: Option<String>,
    discovery: SharedDiscovery,
) -> Result<(), String> {
    let _transition = BRIDGE_TRANSITION
        .lock()
        .map_err(|_| "bridge lifecycle lock poisoned".to_string())?;
    code = normalize_pairing_code(&code);
    if code.is_empty() {
        // Last resort: generate a 6-digit code so host can still start.
        code = format!("{:06}", (std::process::id() % 900_000) + 100_000);
    }
    claim_bridge()?;
    let device_id = device_id.unwrap_or_else(|| with_status(&shared, |s| s.device_id.clone()));
    with_status(&shared, |s| {
        s.phase = HubPhase::WaitingPeer;
        s.role = Some("host".into());
        s.last_role = Some("host".into());
        s.device_id = device_id.clone();
        s.pairing_code = Some(code.clone());
        s.listen_port = port;
        s.endpoint = Some(format!("0.0.0.0:{port}"));
        s.last_error = None;
        s.peer_device = None;
        s.reconnect_attempt = 0;
        s.connection = Some(ConnectionState::Disconnected);
    });
    persist_status_config(&shared);

    if let Some(d) = discovery_handle(&discovery).as_ref() {
        if let Err(err) = d.advertise(&device_id, port) {
            eprintln!("mdns_advertise_error={err}");
        }
    }

    thread::spawn(move || {
        run_with_reconnect(
            shared,
            BridgeJob::Listen {
                code,
                port,
                device_id,
            },
            discovery,
        );
    });
    Ok(())
}

fn start_connect(
    shared: SharedStatus,
    mut code: String,
    addr: String,
    device_id: Option<String>,
    discovery: SharedDiscovery,
) -> Result<(), String> {
    let _transition = BRIDGE_TRANSITION
        .lock()
        .map_err(|_| "bridge lifecycle lock poisoned".to_string())?;
    code = normalize_pairing_code(&code);
    let addr = addr.trim().to_string();
    if code.is_empty() {
        return Err("code required".into());
    }
    if addr.is_empty() {
        return Err("addr required".into());
    }
    claim_bridge()?;
    let device_id = device_id.unwrap_or_else(|| with_status(&shared, |s| s.device_id.clone()));
    with_status(&shared, |s| {
        s.phase = HubPhase::Pairing;
        s.role = Some("joiner".into());
        s.last_role = Some("joiner".into());
        s.device_id = device_id.clone();
        s.pairing_code = Some(code.clone());
        s.connect_addr = Some(addr.clone());
        s.endpoint = Some(addr.clone());
        s.last_error = None;
        s.peer_device = None;
        s.reconnect_attempt = 0;
        s.connection = Some(ConnectionState::Pairing);
    });
    persist_status_config(&shared);

    // Joiner does not advertise; ensure any leftover host ad is stopped.
    if let Some(d) = discovery_handle(&discovery).as_ref() {
        d.stop_advertise();
    }

    thread::spawn(move || {
        run_with_reconnect(
            shared,
            BridgeJob::Connect {
                code,
                addr,
                device_id,
            },
            discovery,
        );
    });
    Ok(())
}

enum BridgeJob {
    Listen {
        code: String,
        port: u16,
        device_id: String,
    },
    Connect {
        code: String,
        addr: String,
        device_id: String,
    },
}

fn run_with_reconnect(shared: SharedStatus, job: BridgeJob, discovery: SharedDiscovery) {
    run_with_reconnect_with_timeouts(
        shared,
        job,
        discovery,
        PAIRING_TIMEOUT,
        CONNECT_ATTEMPT_TIMEOUT,
    );
}

fn run_with_reconnect_with_timeouts(
    shared: SharedStatus,
    job: BridgeJob,
    discovery: SharedDiscovery,
    pairing_timeout: Duration,
    connect_attempt_timeout: Duration,
) {
    let mut attempt: u32 = 0;
    let mut ever_connected = false;
    let initial_join_deadline = match &job {
        BridgeJob::Connect { .. } => Some(Instant::now() + pairing_timeout),
        BridgeJob::Listen { .. } => None,
    };
    let mut terminal_error = None;

    loop {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
            break;
        }

        let pairing_deadline = match (&job, ever_connected) {
            (BridgeJob::Connect { .. }, false) => initial_join_deadline,
            (BridgeJob::Connect { .. }, true) => Some(Instant::now() + pairing_timeout),
            (BridgeJob::Listen { .. }, _) => None,
        };
        if pairing_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            terminal_error = Some(pairing_timeout_error(pairing_timeout));
            break;
        }

        with_status(&shared, |s| {
            s.reconnect_attempt = attempt;
            s.last_error = if attempt == 0 {
                None
            } else {
                Some(format!("reconnect attempt {attempt}"))
            };
            match &job {
                BridgeJob::Listen { .. } => {
                    s.phase = HubPhase::WaitingPeer;
                    s.role = Some("host".into());
                    s.connection = Some(ConnectionState::Disconnected);
                }
                BridgeJob::Connect { .. } => {
                    s.phase = HubPhase::Pairing;
                    s.role = Some("joiner".into());
                    s.connection = Some(ConnectionState::Pairing);
                }
            }
        });

        let result = match &job {
            BridgeJob::Listen {
                code,
                port,
                device_id,
            } => listen_worker(
                shared.clone(),
                code.clone(),
                *port,
                device_id.clone(),
                pairing_timeout,
            ),
            BridgeJob::Connect {
                code,
                addr,
                device_id,
            } => connect_worker(
                shared.clone(),
                code.clone(),
                addr.clone(),
                device_id.clone(),
                pairing_deadline.expect("connect jobs always have a pairing deadline"),
                pairing_timeout,
                connect_attempt_timeout,
            ),
        };
        let connected_this_run = with_status(&shared, |s| s.phase == HubPhase::Connected);
        ever_connected |= connected_this_run;
        if connected_this_run {
            attempt = 0;
        }
        {
            let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
            pending.clear();
        }

        if STOP_BRIDGE.load(Ordering::SeqCst) {
            break;
        }
        if !ever_connected
            && initial_join_deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            terminal_error = Some(pairing_timeout_error(pairing_timeout));
            break;
        }

        match result {
            Ok(()) => {
                // Clean stop from worker (e.g. accept loop saw STOP) or rare clean exit.
                break;
            }
            Err(err) => {
                let friendly = humanize_bridge_error(&err);
                // Version skew / bad pair code / timeout cannot self-heal by reconnecting.
                if should_stop_reconnecting(&err, ever_connected) {
                    terminal_error = Some(friendly);
                    break;
                }
                let auto = with_status(&shared, |s| s.auto_reconnect);
                if !auto {
                    terminal_error = Some(friendly);
                    break;
                }
                attempt = attempt.saturating_add(1);
                let delay_secs = reconnect_delay_secs(attempt);
                with_status(&shared, |s| {
                    s.phase = HubPhase::Pairing;
                    s.last_error = Some(format!(
                        "disconnected ({friendly}); auto-reconnect in {delay_secs}s (#{attempt})"
                    ));
                    s.connection = Some(ConnectionState::Disconnected);
                    s.peer_device = None;
                    s.reconnect_attempt = attempt;
                });
                let mut retry_delay = Duration::from_secs(delay_secs);
                if !ever_connected {
                    if let Some(deadline) = initial_join_deadline {
                        retry_delay =
                            retry_delay.min(deadline.saturating_duration_since(Instant::now()));
                    }
                }
                if retry_delay.is_zero() {
                    terminal_error = Some(pairing_timeout_error(pairing_timeout));
                    break;
                }
                if !sleep_interruptible(retry_delay) {
                    break;
                }
                if !ever_connected
                    && initial_join_deadline.is_some_and(|deadline| Instant::now() >= deadline)
                {
                    terminal_error = Some(pairing_timeout_error(pairing_timeout));
                    break;
                }
            }
        }
    }

    BRIDGE_STOPPING.store(true, Ordering::SeqCst);
    if let Some(d) = discovery_handle(&discovery).as_ref() {
        d.stop_advertise();
    }
    BRIDGE_RUNNING.store(false, Ordering::SeqCst);
    with_status(&shared, |s| {
        s.phase = if terminal_error.is_some() {
            HubPhase::Error
        } else {
            HubPhase::Idle
        };
        s.connection = Some(ConnectionState::Disconnected);
        s.peer_device = None;
        s.reconnect_attempt = 0;
        s.last_error = terminal_error;
        if s.phase == HubPhase::Idle {
            s.role = None;
        }
    });
    BRIDGE_STOPPING.store(false, Ordering::SeqCst);
}

fn pairing_timeout_error(timeout: Duration) -> String {
    let timeout_label = if timeout.subsec_nanos() == 0 {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    };
    format!(
        "pairing timeout ({timeout_label}): 请确认对端已开始等待/连接、配对码一致、防火墙放行端口"
    )
}

fn humanize_bridge_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if lower.contains("pairing rejected") || lower.contains("pairing code mismatch") {
        return format!("配对码错误或已过期（两端请使用同一 6 位码）。详情：{err}");
    }
    if lower.contains("pairing timeout") {
        return err.to_string();
    }
    if lower.contains("unexpected peer device id") {
        return format!("两端 device_id 冲突或异常（设置里改成本机唯一名称后重试）。详情：{err}");
    }
    if lower.contains("unknown message type") {
        let ty = lower
            .split("unknown message type")
            .nth(1)
            .and_then(|s| {
                let digits: String = s
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if digits.is_empty() {
                    None
                } else {
                    Some(digits)
                }
            })
            .unwrap_or_else(|| "?".into());
        return format!(
            "协议不兼容：未知消息类型 {ty}（11+=文件通道）。请 Linux 与 Windows 两端同时升级到同一版本后重连：git pull && cargo build -p m590-ui。详情：{err}"
        );
    }
    if lower.contains("unsupported protocol version") {
        return format!("协议版本不匹配，请两端升级到同一版本。详情：{err}");
    }
    err.to_string()
}

fn is_protocol_mismatch(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("unknown message type") || lower.contains("unsupported protocol version")
}

fn is_pairing_timeout(err: &str) -> bool {
    err.to_ascii_lowercase().contains("pairing timeout")
}

fn should_stop_reconnecting(err: &str, ever_connected: bool) -> bool {
    is_protocol_mismatch(err)
        || (is_non_retriable_pair_error(err) && !(ever_connected && is_pairing_timeout(err)))
}

/// Initial-pairing failures that will not heal by blind reconnect.
fn is_non_retriable_pair_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("pairing rejected")
        || lower.contains("pairing code mismatch")
        || lower.contains("pairing timeout")
        || lower.contains("unexpected peer device id")
        || lower.contains("pairing failed")
}

fn reconnect_delay_secs(attempt: u32) -> u64 {
    // 1,2,4,8,16,30,30...
    let shift = attempt.saturating_sub(1).min(5);
    (1u64 << shift).min(30)
}

fn sleep_interruptible(total: Duration) -> bool {
    let slice = Duration::from_millis(100);
    let mut left = total;
    while left > Duration::ZERO {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
            return false;
        }
        let step = if left > slice { slice } else { left };
        thread::sleep(step);
        left = left.saturating_sub(step);
    }
    !STOP_BRIDGE.load(Ordering::SeqCst)
}

fn push_text(shared: &SharedStatus, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("text required".into());
    }
    let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
    let phase = with_status(shared, |s| s.phase);
    if phase != HubPhase::Connected {
        return Err("not connected".into());
    }
    if pending.text.is_some() {
        return Err("text push already pending".into());
    }
    pending.text = Some(text);
    Ok(())
}

fn push_file(shared: &SharedStatus, path: String) -> Result<(), String> {
    if path.is_empty() {
        return Err("path required".into());
    }
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(format!("not a file: {path}"));
    }
    let meta = fs::metadata(&p).map_err(|e| format!("stat file: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "file too large: {}B > limit {MAX_FILE_BYTES}B",
            meta.len()
        ));
    }
    let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
    let phase = with_status(shared, |s| s.phase);
    if phase != HubPhase::Connected {
        return Err("not connected".into());
    }
    if pending.file_path.is_some() || pending.batch.is_some() {
        return Err("file send already pending".into());
    }
    pending.file_path = Some(path);
    Ok(())
}

struct PendingCommands {
    text: Option<String>,
    file_path: Option<String>,
    file_bytes: Option<(String, Vec<u8>)>,
    batch: Option<PreparedBatch>,
    cancel_batch: bool,
}

impl PendingCommands {
    const fn new() -> Self {
        Self {
            text: None,
            file_path: None,
            file_bytes: None,
            batch: None,
            cancel_batch: false,
        }
    }

    fn clear(&mut self) {
        self.text = None;
        self.file_path = None;
        self.file_bytes = None;
        self.batch = None;
        self.cancel_batch = false;
    }
}

static PENDING_COMMANDS: std::sync::Mutex<PendingCommands> =
    std::sync::Mutex::new(PendingCommands::new());

fn clear_pending_commands() {
    PENDING_COMMANDS
        .lock()
        .expect("pending commands lock")
        .clear();
}

fn push_file_bytes(shared: &SharedStatus, name: String, data_b64: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("name required".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err("name must be a basename".into());
    }
    if data_b64.is_empty() {
        return Err("data_base64 required".into());
    }
    let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
    let phase = with_status(shared, |s| s.phase);
    if phase != HubPhase::Connected {
        return Err("not connected".into());
    }
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64.trim())
        .map_err(|e| format!("base64 decode: {e}"))?;
    if data.len() > MAX_HTTP_FILE_BYTES {
        return Err(format!(
            "file too large for bytes API: {}B > limit {MAX_HTTP_FILE_BYTES}B (use /api/send_file path)",
            data.len()
        ));
    }
    if pending.file_bytes.is_some() || pending.batch.is_some() {
        return Err("file bytes send already pending".into());
    }
    pending.file_bytes = Some((name, data));
    Ok(())
}

#[derive(Debug)]
struct ScannedBatchEntry {
    relative_path: String,
    kind: BatchEntryKind,
    size: u64,
    source: Option<PathBuf>,
}

fn push_batch(shared: &SharedStatus, paths: Vec<String>) -> Result<(), String> {
    if with_status(shared, |s| s.phase) != HubPhase::Connected {
        return Err("not connected".into());
    }
    let prepared = scan_batch_paths(paths)?;
    let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
    if pending.file_path.is_some() || pending.file_bytes.is_some() || pending.batch.is_some() {
        return Err("file send already pending".into());
    }
    pending.batch = Some(prepared);
    Ok(())
}

fn queue_batch_cancel(shared: &SharedStatus) -> Result<(), String> {
    if with_status(shared, |s| s.phase) != HubPhase::Connected {
        return Err("not connected".into());
    }
    let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
    pending.batch = None;
    pending.cancel_batch = true;
    Ok(())
}

fn file_list_requires_batch(paths: &[PathBuf]) -> bool {
    paths.len() > 1 || paths.first().is_some_and(|path| path.is_dir())
}

fn scan_batch_paths(paths: Vec<String>) -> Result<PreparedBatch, String> {
    if paths.is_empty() {
        return Err("paths must contain at least one file or directory".into());
    }
    if paths.len() > MAX_BATCH_ENTRIES {
        return Err(format!(
            "too many selected paths: {} > {MAX_BATCH_ENTRIES}",
            paths.len()
        ));
    }

    let batch_seq = NEXT_BATCH_ID.fetch_add(1, Ordering::Relaxed);
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| format!("generate batch id: {error}"))?;
    let batch_id = format!("batch-{}-{batch_seq}", m590_core::bytes_to_hex(&nonce));
    let mut roots = Vec::with_capacity(paths.len());
    for raw in paths {
        if raw.is_empty() {
            return Err("selected path must not be empty".into());
        }
        let path = PathBuf::from(&raw);
        let metadata = fs::symlink_metadata(&path).map_err(|err| format!("stat {raw}: {err}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("symbolic links are not supported: {raw}"));
        }
        let root_name = utf8_file_name(&path)?;
        roots.push((root_name, path, metadata));
    }
    roots.sort_by(|left, right| left.0.cmp(&right.0));

    let display_name = if roots.len() == 1 {
        roots[0].0.clone()
    } else {
        format!("{} items", roots.len())
    };
    let mut scanned = Vec::new();
    for (root_name, path, metadata) in roots {
        if metadata.is_file() {
            scan_regular_file(&path, root_name, metadata.len(), &mut scanned)?;
        } else if metadata.is_dir() {
            scanned.push(ScannedBatchEntry {
                relative_path: root_name.clone(),
                kind: BatchEntryKind::Directory,
                size: 0,
                source: None,
            });
            scan_directory(&path, &root_name, 1, &mut scanned)?;
        } else {
            return Err(format!("unsupported filesystem entry: {}", path.display()));
        }
        if scanned.len() > MAX_BATCH_ENTRIES {
            return Err(format!(
                "batch contains too many entries: {} > {MAX_BATCH_ENTRIES}",
                scanned.len()
            ));
        }
    }
    scanned.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut entries = Vec::with_capacity(scanned.len());
    let mut sources = Vec::new();
    for (index, item) in scanned.into_iter().enumerate() {
        let entry_id = format!("{batch_id}-entry-{}", index + 1);
        let entry = BatchEntry::new(
            entry_id.clone(),
            item.relative_path,
            item.kind,
            item.size,
            "",
        )
        .map_err(|err| err.to_string())?;
        if let Some(path) = item.source {
            sources.push(BatchFileSource::new(entry_id, path));
        }
        entries.push(entry);
    }

    FileBatchOfferPayload::new(
        DeviceId::new("local-batch-scan"),
        batch_id.clone(),
        display_name.clone(),
        entries.clone(),
    )
    .map_err(|err| err.to_string())?;
    Ok(PreparedBatch {
        batch_id,
        display_name,
        entries,
        sources,
    })
}

fn scan_directory(
    directory: &Path,
    relative_directory: &str,
    depth: usize,
    scanned: &mut Vec<ScannedBatchEntry>,
) -> Result<(), String> {
    if depth >= MAX_BATCH_PATH_DEPTH {
        return Err(format!(
            "directory tree exceeds depth limit {MAX_BATCH_PATH_DEPTH}: {}",
            directory.display()
        ));
    }
    let mut children = Vec::new();
    for child in fs::read_dir(directory)
        .map_err(|err| format!("read directory {}: {err}", directory.display()))?
    {
        let child = child.map_err(|err| format!("read directory entry: {err}"))?;
        let name = child
            .file_name()
            .to_str()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("non-UTF-8 file name under {}", directory.display()))?
            .to_string();
        children.push((name, child.path()));
    }
    children.sort_by(|left, right| left.0.cmp(&right.0));

    for (name, path) in children {
        let metadata =
            fs::symlink_metadata(&path).map_err(|err| format!("stat {}: {err}", path.display()))?;
        // Never follow directory or file symlinks. Nested links are deliberately omitted.
        if metadata.file_type().is_symlink() {
            continue;
        }
        let relative_path = format!("{relative_directory}/{name}");
        if metadata.is_dir() {
            scanned.push(ScannedBatchEntry {
                relative_path: relative_path.clone(),
                kind: BatchEntryKind::Directory,
                size: 0,
                source: None,
            });
            if scanned.len() > MAX_BATCH_ENTRIES {
                return Err(format!(
                    "batch contains too many entries: {} > {MAX_BATCH_ENTRIES}",
                    scanned.len()
                ));
            }
            scan_directory(&path, &relative_path, depth + 1, scanned)?;
        } else if metadata.is_file() {
            scan_regular_file(&path, relative_path, metadata.len(), scanned)?;
        } else {
            return Err(format!("unsupported filesystem entry: {}", path.display()));
        }
    }
    Ok(())
}

fn scan_regular_file(
    path: &Path,
    relative_path: String,
    size: u64,
    scanned: &mut Vec<ScannedBatchEntry>,
) -> Result<(), String> {
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "file too large: {} is {size}B > limit {MAX_FILE_BYTES}B",
            path.display()
        ));
    }
    scanned.push(ScannedBatchEntry {
        relative_path,
        kind: BatchEntryKind::File,
        size,
        source: Some(path.to_path_buf()),
    });
    Ok(())
}

fn utf8_file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("path has no UTF-8 file name: {}", path.display()))
}

fn listen_worker(
    shared: SharedStatus,
    code: String,
    port: u16,
    device_id: String,
    pairing_timeout: Duration,
) -> Result<(), String> {
    let bind = format!("0.0.0.0:{port}");
    let listener = listen_on(&bind).map_err(|e| e.to_string())?;
    with_status(&shared, |s| {
        s.phase = HubPhase::WaitingPeer;
        s.endpoint = Some(bind);
        s.listen_port = port;
        s.connection = Some(ConnectionState::Disconnected);
    });
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut conn = loop {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
            return Ok(());
        }
        match accept_framed(&listener) {
            Ok(c) => break c,
            Err(m590_net::TcpError::Io(err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(err.to_string()),
        }
    };
    listener.set_nonblocking(false).ok();

    let mut session = Session::new(DeviceId::new(device_id)).map_err(|e| e.to_string())?;
    session
        .handle(SessionEvent::StartPairing {
            expected_code: code,
        })
        .map_err(|e| e.to_string())?;
    let _ = session.take_outbox();
    with_status(&shared, |s| {
        s.phase = HubPhase::Pairing;
        s.connection = Some(ConnectionState::Pairing);
    });
    run_session_loop(
        shared,
        &mut session,
        &mut conn,
        Instant::now() + pairing_timeout,
        pairing_timeout,
    )
}

fn connect_worker(
    shared: SharedStatus,
    code: String,
    addr: String,
    device_id: String,
    pairing_deadline: Instant,
    pairing_timeout: Duration,
    connect_attempt_timeout: Duration,
) -> Result<(), String> {
    let remaining = pairing_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(pairing_timeout_error(pairing_timeout));
    }
    let mut conn = connect_framed_timeout(&addr, remaining.min(connect_attempt_timeout))
        .map_err(|e| e.to_string())?;
    let mut session = Session::new(DeviceId::new(device_id)).map_err(|e| e.to_string())?;
    session
        .handle(SessionEvent::StartPairing {
            expected_code: code,
        })
        .map_err(|e| e.to_string())?;
    conn.send_all(session.take_outbox().iter())
        .map_err(|e| e.to_string())?;
    run_session_loop(
        shared,
        &mut session,
        &mut conn,
        pairing_deadline,
        pairing_timeout,
    )
}

fn run_session_loop(
    shared: SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    pairing_deadline: Instant,
    pairing_timeout: Duration,
) -> Result<(), String> {
    let mut clipboard = PlatformClipboard::open().ok();
    let mut last_heartbeat = Instant::now();
    let mut last_peer_rx = Instant::now();
    let mut content_seq = 0u64;
    let mut latest_clipboard_file_offer_id: Option<String> = None;
    let mut queued_file_status: Option<QueuedFileStatus> = None;
    let mut outbound_batch: Option<OutboundBatchState> = None;
    let mut inbound_batch: Option<InboundBatchState> = None;
    #[cfg(target_os = "windows")]
    let ole_manager = WindowsVirtualFileManager::start().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let mut virtual_receive: Option<WindowsVirtualReceive> = None;
    #[cfg(target_os = "windows")]
    let mut virtual_batch_receive: Option<WindowsVirtualBatchReceive> = None;
    #[cfg(target_os = "windows")]
    let mut deferred_virtual_offer: Option<DeferredVirtualOffer> = None;
    #[cfg(target_os = "windows")]
    let mut deferred_virtual_batch_offer: Option<DeferredVirtualBatchOffer> = None;
    #[cfg(target_os = "linux")]
    let mut fuse_manager = LinuxVirtualFileManager::new();
    #[cfg(target_os = "linux")]
    let mut virtual_receive: Option<LinuxVirtualReceive> = None;
    #[cfg(target_os = "linux")]
    let mut virtual_batch_receive: Option<LinuxVirtualBatchReceive> = None;
    #[cfg(target_os = "linux")]
    let mut deferred_virtual_offer: Option<DeferredVirtualOffer> = None;
    #[cfg(target_os = "linux")]
    let mut deferred_virtual_batch_offer: Option<DeferredVirtualBatchOffer> = None;
    #[cfg(target_os = "linux")]
    let mut pending_virtual_chunk: Option<PendingLinuxVirtualChunk> = None;
    while session.state() != ConnectionState::Connected {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
            let _ = session.handle(SessionEvent::Disconnect);
            return Ok(());
        }
        if Instant::now() >= pairing_deadline {
            let _ = session.handle(SessionEvent::Disconnect);
            return Err(pairing_timeout_error(pairing_timeout));
        }
        let read_timeout = pairing_deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(200));
        if read_timeout.is_zero() {
            let _ = session.handle(SessionEvent::Disconnect);
            return Err(pairing_timeout_error(pairing_timeout));
        }
        conn.set_read_timeout(Some(read_timeout))
            .map_err(|e| e.to_string())?;
        conn.set_nonblocking(false).map_err(|e| e.to_string())?;
        match conn.recv() {
            Ok(msg) => {
                // Drain reject/outbox even when handle returns PairRejected.
                let handle_result = session.handle(SessionEvent::Message(msg));
                let send_result = conn.send_all(session.take_outbox().iter());
                send_result.map_err(|e| e.to_string())?;
                handle_result.map_err(|e| e.to_string())?;
                last_peer_rx = Instant::now();
                if session.state() == ConnectionState::Disconnected {
                    return Err("pairing failed: session disconnected".into());
                }
            }
            Err(m590_net::TcpError::Io(err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(m590_net::TcpError::Disconnected) => {
                return Err("peer disconnected during pairing".into())
            }
            Err(err) => return Err(err.to_string()),
        }
    }

    with_status(&shared, |s| {
        s.phase = HubPhase::Connected;
        s.connection = Some(ConnectionState::Connected);
        s.peer_device = session.peer_device().map(|d| d.as_str().to_string());
        s.last_error = None;
        s.reconnect_attempt = 0;
    });

    // Pairing may have taken seconds; re-arm polls so an already-copied image file
    // is observed once after Connected (instead of being treated as baseline).
    if let Some(clip) = clipboard.as_mut() {
        clip.prime_poll_to_emit_current();
    }

    // Inbound .part files land under save_dir/.partial then finalize into save_dir.
    {
        let save_dir = with_status(&shared, |s| PathBuf::from(s.file_save_dir.clone()));
        session.set_file_receive_dir(save_dir.join(".partial"));
    }

    loop {
        let mut file_progressed = false;

        if STOP_BRIDGE.load(Ordering::SeqCst) {
            let _ = session.handle(SessionEvent::Disconnect);
            with_status(&shared, |s| {
                s.phase = HubPhase::Idle;
                s.connection = Some(ConnectionState::Disconnected);
                s.peer_device = None;
            });
            return Ok(());
        }

        if last_peer_rx.elapsed() > Duration::from_secs(15) {
            return Err("peer idle timeout".into());
        }
        if session.peer_heartbeat_suspect(DEFAULT_HEARTBEAT_MISS_THRESHOLD) {
            return Err("peer heartbeat suspect".into());
        }

        if last_heartbeat.elapsed() >= Duration::from_secs(2) {
            session
                .handle(SessionEvent::HeartbeatTick)
                .map_err(|e| e.to_string())?;
            conn.send_all(session.take_outbox().iter())
                .map_err(|e| e.to_string())?;
            last_heartbeat = Instant::now();
        }

        let cancel_batch = {
            let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
            std::mem::take(&mut pending.cancel_batch)
        };
        if cancel_batch {
            cancel_runtime_batch(
                &shared,
                session,
                conn,
                &mut outbound_batch,
                &mut inbound_batch,
                "cancelled by local user",
            )?;
            #[cfg(target_os = "linux")]
            {
                let mut cancelled_virtual_batch = None;
                if let Some(current) = virtual_batch_receive.take() {
                    cancelled_virtual_batch = Some(current.batch_id.clone());
                    cancel_linux_virtual_batch(session, conn, &current, "cancelled by local user")?;
                }
                if let Some(deferred) = deferred_virtual_batch_offer.take() {
                    cancelled_virtual_batch = Some(deferred.batch_id.clone());
                    cancel_deferred_linux_virtual_batch(
                        session,
                        conn,
                        deferred,
                        "cancelled by local user",
                    )?;
                }
                if let Some(batch_id) = cancelled_virtual_batch {
                    fuse_manager.clear();
                    with_status(&shared, |status| {
                        status.file_transfer_phase = Some("cancelled".into());
                        status.last_file_transfer_id = Some(batch_id);
                        status.file_batch_current_path = None;
                        status.file_bytes_received = None;
                        status.file_bytes_total = None;
                        status.last_error = None;
                    });
                }
            }
        }

        #[cfg(target_os = "linux")]
        if let Some(pending) = pending_virtual_chunk.take() {
            match try_push_linux_virtual_chunk(
                &pending.transfer_id,
                &pending.data,
                virtual_receive.as_ref(),
                virtual_batch_receive.as_ref(),
            ) {
                Ok(LinuxVirtualChunkPush::Accepted | LinuxVirtualChunkPush::NoReceiver) => {}
                Ok(LinuxVirtualChunkPush::Backpressured) => {
                    pending_virtual_chunk = Some(pending);
                }
                Err(error) => {
                    with_status(&shared, |status| {
                        status.last_error = Some(format!("virtual file stream: {error}"));
                    });
                    pending_virtual_chunk = Some(pending);
                }
            }
        }

        let pending_text = PENDING_COMMANDS
            .lock()
            .expect("pending commands lock")
            .text
            .take();
        if let Some(text) = pending_text {
            latest_clipboard_file_offer_id = None;
            content_seq += 1;
            let cid = format!("ui-push-{}-{content_seq}", std::process::id());
            if let Ok(QueueClipboardResult::Queued) =
                session.queue_clipboard_text(cid, text.clone())
            {
                conn.send_all(session.take_outbox().iter())
                    .map_err(|e| e.to_string())?;
                with_status(&shared, |s| {
                    s.last_sync_text = Some(text.clone());
                });
                if let Some(clip) = clipboard.as_mut() {
                    let _ = clip.write_text(&text);
                }
            }
        }

        let pending_file = PENDING_COMMANDS
            .lock()
            .expect("pending commands lock")
            .file_path
            .take();
        if let Some(path) = pending_file {
            cancel_runtime_batch(
                &shared,
                session,
                conn,
                &mut outbound_batch,
                &mut inbound_batch,
                "replaced by a single-file send",
            )?;
            match offer_local_file(session, &mut content_seq, &path) {
                Ok((summary, transfer_id, file_name, bytes)) => {
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                    queue_or_mark_file_sending(
                        &shared,
                        session.outbound_file_progress().is_some(),
                        &mut queued_file_status,
                        QueuedFileStatus {
                            summary,
                            transfer_id,
                            file_name,
                            bytes,
                        },
                    );
                }
                Err(err) => {
                    with_status(&shared, |s| {
                        s.file_transfer_phase = Some("failed".into());
                        s.last_error = Some(err);
                    });
                }
            }
        }

        let pending_file_bytes = PENDING_COMMANDS
            .lock()
            .expect("pending commands lock")
            .file_bytes
            .take();
        if let Some((name, data)) = pending_file_bytes {
            cancel_runtime_batch(
                &shared,
                session,
                conn,
                &mut outbound_batch,
                &mut inbound_batch,
                "replaced by a single-file send",
            )?;
            match offer_file_bytes(session, &mut content_seq, name, data) {
                Ok((summary, transfer_id, file_name, bytes)) => {
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                    queue_or_mark_file_sending(
                        &shared,
                        session.outbound_file_progress().is_some(),
                        &mut queued_file_status,
                        QueuedFileStatus {
                            summary,
                            transfer_id,
                            file_name,
                            bytes,
                        },
                    );
                }
                Err(err) => {
                    with_status(&shared, |s| {
                        s.file_transfer_phase = Some("failed".into());
                        s.last_error = Some(err);
                    });
                }
            }
        }

        let pending_batch = PENDING_COMMANDS
            .lock()
            .expect("pending commands lock")
            .batch
            .take();
        if let Some(prepared) = pending_batch {
            cancel_runtime_batch(
                &shared,
                session,
                conn,
                &mut outbound_batch,
                &mut inbound_batch,
                "replaced by a newer batch",
            )?;
            let state = OutboundBatchState::from_prepared(&prepared);
            let entry_count = prepared.entries.len();
            let directory_count = prepared
                .entries
                .iter()
                .filter(|entry| entry.kind == BatchEntryKind::Directory)
                .count();
            task_057_diagnostic(format_args!(
                "batch_offer_dispatch batch_id={:?} entries={} files={} directories={directory_count}",
                prepared.batch_id,
                entry_count,
                state.files.len()
            ));
            match session.offer_file_batch_paths(
                prepared.batch_id.clone(),
                prepared.display_name.clone(),
                prepared.entries,
                prepared.sources,
            ) {
                Ok(QueueFileResult::Queued) => {
                    let outbox = session.take_outbox();
                    conn.send_all(outbox.iter()).map_err(|e| e.to_string())?;
                    task_057_diagnostic(format_args!(
                        "batch_offer_sent batch_id={:?} entries={} files={} directories={directory_count}",
                        prepared.batch_id,
                        entry_count,
                        state.files.len()
                    ));
                    mark_outbound_batch_started(&shared, &state);
                    if state.files.is_empty() {
                        mark_batch_done(&shared, &state.batch_id, None);
                    } else {
                        outbound_batch = Some(state);
                    }
                }
                Ok(other) => {
                    with_status(&shared, |status| {
                        mark_batch_failed(
                            status,
                            &prepared.batch_id,
                            &format!("queue batch failed: {other:?}"),
                        );
                    });
                }
                Err(err) => {
                    with_status(&shared, |status| {
                        mark_batch_failed(status, &prepared.batch_id, &err.to_string());
                    });
                }
            }
        }

        // Stream more file chunks without starving the rest of the loop forever.
        if session.has_pending_outbound_file() {
            session.pump_outbound_file().map_err(|e| e.to_string())?;
            file_progressed = true;
            let outbox = session.take_outbox();
            conn.send_all(outbox.iter()).map_err(|e| e.to_string())?;
            let resolutions =
                note_outbound_file_completes(&shared, &outbox, &mut queued_file_status);
            note_outbound_batch_completes(&shared, session, conn, &outbox, &mut outbound_batch)?;
            rearm_completed_clipboard_offers(
                clipboard.as_mut(),
                &mut latest_clipboard_file_offer_id,
                &resolutions,
            );
            if let Some((tid, sent, total)) = session.outbound_file_progress() {
                with_status(&shared, |s| {
                    if s.last_file_transfer_id.as_deref() == Some(tid.as_str()) {
                        s.file_transfer_phase = Some("sending".into());
                        s.file_bytes_received = Some(sent);
                        s.file_bytes_total = Some(total);
                    }
                });
            }
        }

        loop {
            #[cfg(target_os = "linux")]
            if pending_virtual_chunk.is_some() {
                break;
            }
            match conn.try_recv() {
                Ok(Some(msg)) => {
                    file_progressed |=
                        matches!(msg, Message::FileChunk(_) | Message::FileComplete(_));
                    note_outbound_batch_request(&shared, &msg, outbound_batch.as_ref());
                    session
                        .handle(SessionEvent::Message(msg))
                        .map_err(|e| e.to_string())?;
                    last_peer_rx = Instant::now();
                    let outbox = session.take_outbox();
                    conn.send_all(outbox.iter()).map_err(|e| e.to_string())?;
                    let resolutions =
                        note_outbound_file_completes(&shared, &outbox, &mut queued_file_status);
                    note_outbound_batch_completes(
                        &shared,
                        session,
                        conn,
                        &outbox,
                        &mut outbound_batch,
                    )?;
                    rearm_completed_clipboard_offers(
                        clipboard.as_mut(),
                        &mut latest_clipboard_file_offer_id,
                        &resolutions,
                    );
                    match session.take_inbound_clipboard() {
                        Some(InboundClipboardResult::Applied { content_id, text }) => {
                            latest_clipboard_file_offer_id = None;
                            with_status(&shared, |s| {
                                s.last_sync_text = Some(text.clone());
                                s.last_sync_content_id = Some(content_id);
                            });
                            if let Some(clip) = clipboard.as_mut() {
                                let _ = clip.write_text(&text);
                            }
                        }
                        Some(InboundClipboardResult::AppliedImage {
                            content_id,
                            width,
                            height,
                            encoding,
                            data,
                        }) => {
                            latest_clipboard_file_offer_id = None;
                            let summary =
                                format!("[image {width}x{height} {}B {encoding:?}]", data.len());
                            with_status(&shared, |s| {
                                s.last_sync_content_id = Some(content_id);
                                s.last_sync_text = Some(summary);
                                s.last_error = None;
                            });
                            if let Some(clip) = clipboard.as_mut() {
                                match m590_clipboard::ImageClipboard::from_wire(
                                    width, height, encoding, data,
                                ) {
                                    Ok(image) => {
                                        if let Err(err) = clip.write_image(&image) {
                                            with_status(&shared, |s| {
                                                s.last_error =
                                                    Some(format!("clipboard_write_image: {err}"));
                                            });
                                        }
                                    }
                                    Err(err) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!("image_decode: {err}"));
                                        });
                                    }
                                }
                            }
                        }
                        Some(InboundClipboardResult::DuplicateContentId) | None => {}
                    }

                    if let Some(file_event) = session.take_inbound_file() {
                        #[cfg(target_os = "windows")]
                        if let InboundFileResult::BatchOffered {
                            batch_id,
                            display_name,
                            entries,
                        } = &file_event
                        {
                            #[cfg(feature = "task-057-diagnostics")]
                            {
                                let file_count = entries
                                    .iter()
                                    .filter(|entry| entry.kind == BatchEntryKind::File)
                                    .count();
                                let directory_count = entries.len().saturating_sub(file_count);
                                let total_bytes = entries
                                    .iter()
                                    .filter(|entry| entry.kind == BatchEntryKind::File)
                                    .map(|entry| entry.size)
                                    .sum::<u64>();
                                task_057_diagnostic(format_args!(
                                    "batch_received batch_id={batch_id:?} display_name={display_name:?} entries={} files={file_count} directories={directory_count} total_bytes={total_bytes}",
                                    entries.len()
                                ));
                                for (descriptor_index, entry) in entries.iter().enumerate() {
                                    task_057_diagnostic(format_args!(
                                        "batch_entry batch_id={batch_id:?} lindex={descriptor_index} kind={:?} entry_id={:?} path={:?} size={}",
                                        entry.kind,
                                        entry.entry_id,
                                        entry.relative_path,
                                        entry.size
                                    ));
                                }
                            }
                            cancel_runtime_batch(
                                &shared,
                                session,
                                conn,
                                &mut outbound_batch,
                                &mut inbound_batch,
                                "replaced by a Windows virtual batch",
                            )?;
                            let next = DeferredVirtualBatchOffer {
                                batch_id: batch_id.clone(),
                                display_name: display_name.clone(),
                                entries: entries.clone(),
                            };
                            let must_defer = virtual_receive.as_ref().is_some_and(|current| {
                                active_virtual_receive_must_finish(
                                    current.requested,
                                    current.completed,
                                )
                            }) || virtual_batch_receive
                                .as_ref()
                                .is_some_and(WindowsVirtualBatchReceive::must_finish);
                            if must_defer {
                                if let Some(stale) = deferred_virtual_offer.take() {
                                    session
                                        .cancel_file(
                                            stale.transfer_id,
                                            "replaced by a newer deferred batch offer",
                                        )
                                        .map_err(|error| error.to_string())?;
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|error| error.to_string())?;
                                }
                                if let Some(stale) = deferred_virtual_batch_offer.take() {
                                    cancel_deferred_windows_virtual_batch(
                                        session,
                                        conn,
                                        stale,
                                        "replaced by a newer deferred batch offer",
                                    )?;
                                }
                                deferred_virtual_batch_offer = Some(next);
                            } else {
                                if let Some(previous) = virtual_receive.take() {
                                    if !previous.completed {
                                        previous.producer.fail("replaced by a newer batch offer");
                                        session
                                            .cancel_file(
                                                previous.transfer_id,
                                                "replaced by a newer batch offer",
                                            )
                                            .map_err(|error| error.to_string())?;
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|error| error.to_string())?;
                                    }
                                }
                                if let Some(previous) = virtual_batch_receive.take() {
                                    cancel_windows_virtual_batch(
                                        session,
                                        conn,
                                        &previous,
                                        "replaced by a newer batch offer",
                                    )?;
                                }
                                if let Some(stale) = deferred_virtual_offer.take() {
                                    session
                                        .cancel_file(
                                            stale.transfer_id,
                                            "replaced by a newer batch offer",
                                        )
                                        .map_err(|error| error.to_string())?;
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|error| error.to_string())?;
                                }
                                if let Some(stale) = deferred_virtual_batch_offer.take() {
                                    cancel_deferred_windows_virtual_batch(
                                        session,
                                        conn,
                                        stale,
                                        "replaced by a newer batch offer",
                                    )?;
                                }
                                ole_manager.clear();
                                match publish_windows_virtual_batch_offer(&ole_manager, &next) {
                                    Ok(receive) => {
                                        virtual_batch_receive = Some(receive);
                                        mark_windows_virtual_batch_offer(&shared, &next);
                                    }
                                    Err(error) => {
                                        cancel_deferred_windows_virtual_batch(
                                            session,
                                            conn,
                                            next,
                                            "cannot publish Windows virtual batch",
                                        )?;
                                        with_status(&shared, |status| {
                                            mark_batch_failed(status, batch_id, &error);
                                        });
                                    }
                                }
                            }
                            continue;
                        }
                        #[cfg(target_os = "linux")]
                        if let InboundFileResult::BatchOffered {
                            batch_id,
                            display_name,
                            entries,
                        } = &file_event
                        {
                            cancel_runtime_batch(
                                &shared,
                                session,
                                conn,
                                &mut outbound_batch,
                                &mut inbound_batch,
                                "replaced by a Linux virtual batch",
                            )?;
                            let next = DeferredVirtualBatchOffer {
                                batch_id: batch_id.clone(),
                                display_name: display_name.clone(),
                                entries: entries.clone(),
                            };
                            let must_defer = virtual_receive.as_ref().is_some_and(|current| {
                                linux_virtual_receive_must_finish(
                                    current.requested,
                                    current.completed,
                                    current.consumed,
                                    current.released,
                                )
                            }) || virtual_batch_receive
                                .as_ref()
                                .is_some_and(LinuxVirtualBatchReceive::must_finish);
                            if must_defer {
                                if let Some(stale) = deferred_virtual_offer.take() {
                                    session
                                        .cancel_file(
                                            stale.transfer_id,
                                            "replaced by a newer deferred batch offer",
                                        )
                                        .map_err(|error| error.to_string())?;
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|error| error.to_string())?;
                                }
                                if let Some(stale) = deferred_virtual_batch_offer.take() {
                                    cancel_deferred_linux_virtual_batch(
                                        session,
                                        conn,
                                        stale,
                                        "replaced by a newer deferred batch offer",
                                    )?;
                                }
                                deferred_virtual_batch_offer = Some(next);
                            } else {
                                if let Some(previous) = virtual_receive.take() {
                                    if !previous.completed {
                                        previous.producer.fail("replaced by a newer batch offer");
                                        session
                                            .cancel_file(
                                                previous.transfer_id,
                                                "replaced by a newer batch offer",
                                            )
                                            .map_err(|error| error.to_string())?;
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|error| error.to_string())?;
                                    }
                                }
                                if let Some(previous) = virtual_batch_receive.take() {
                                    cancel_linux_virtual_batch(
                                        session,
                                        conn,
                                        &previous,
                                        "replaced by a newer batch offer",
                                    )?;
                                }
                                if let Some(stale) = deferred_virtual_offer.take() {
                                    session
                                        .cancel_file(
                                            stale.transfer_id,
                                            "replaced by a newer batch offer",
                                        )
                                        .map_err(|error| error.to_string())?;
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|error| error.to_string())?;
                                }
                                if let Some(stale) = deferred_virtual_batch_offer.take() {
                                    cancel_deferred_linux_virtual_batch(
                                        session,
                                        conn,
                                        stale,
                                        "replaced by a newer batch offer",
                                    )?;
                                }
                                let published = clipboard
                                    .as_mut()
                                    .ok_or_else(|| {
                                        "Linux clipboard unavailable for virtual batch".to_string()
                                    })
                                    .and_then(|clip| {
                                        publish_linux_virtual_batch_offer(
                                            &mut fuse_manager,
                                            clip,
                                            &next,
                                        )
                                    });
                                match published {
                                    Ok(receive) => {
                                        virtual_batch_receive = Some(receive);
                                        mark_linux_virtual_batch_offer(&shared, &next);
                                    }
                                    Err(error) => {
                                        cancel_deferred_linux_virtual_batch(
                                            session,
                                            conn,
                                            next,
                                            "cannot publish Linux virtual batch",
                                        )?;
                                        fuse_manager.clear();
                                        with_status(&shared, |status| {
                                            mark_batch_failed(status, batch_id, &error);
                                        });
                                    }
                                }
                            }
                            continue;
                        }
                        let failed_transfer_id = match &file_event {
                            InboundFileResult::Failed { transfer_id, .. } => {
                                Some(transfer_id.clone())
                            }
                            _ => None,
                        };
                        let cancelled_clipboard_offer = match &file_event {
                            InboundFileResult::Failed {
                                transfer_id,
                                message,
                            } if file_failure_rearms_clipboard_offer(message) => {
                                Some(transfer_id.clone())
                            }
                            _ => None,
                        };
                        #[cfg(target_os = "windows")]
                        if handle_windows_virtual_batch_stream_event(
                            &shared,
                            session,
                            conn,
                            &ole_manager,
                            &file_event,
                            &mut virtual_batch_receive,
                            &mut deferred_virtual_batch_offer,
                        )? {
                            continue;
                        }
                        #[cfg(target_os = "linux")]
                        match handle_linux_virtual_batch_stream_event(
                            &shared,
                            session,
                            conn,
                            &mut fuse_manager,
                            &file_event,
                            &mut virtual_batch_receive,
                            &mut deferred_virtual_batch_offer,
                        )? {
                            LinuxVirtualBatchStreamEvent::Unhandled => {}
                            LinuxVirtualBatchStreamEvent::Handled(pending) => {
                                pending_virtual_chunk = pending;
                                continue;
                            }
                        }
                        let batch_handled = handle_batch_file_event(
                            &shared,
                            session,
                            conn,
                            &file_event,
                            &mut outbound_batch,
                            &mut inbound_batch,
                        )?;
                        if batch_handled {
                            continue;
                        }
                        #[cfg(any(target_os = "linux", target_os = "windows"))]
                        if let Some(current) = virtual_receive.as_mut() {
                            if let InboundFileResult::Chunk { transfer_id, data } = &file_event {
                                if current.transfer_id == *transfer_id {
                                    #[cfg(target_os = "windows")]
                                    if current.first_chunk_at.is_none() {
                                        let first_chunk_at = Instant::now();
                                        current.first_chunk_at = Some(first_chunk_at);
                                        task_057_diagnostic(format_args!(
                                            "single_network_first_chunk entry_id={transfer_id:?} path={:?} bytes={} first_byte_ms={}",
                                            current.file_name,
                                            data.len(),
                                            current
                                                .network_started_at
                                                .map(|started| first_chunk_at
                                                    .saturating_duration_since(started)
                                                    .as_millis())
                                                .unwrap_or(0)
                                        ));
                                    }
                                    #[cfg(target_os = "windows")]
                                    if let Err(error) = current.producer.push(data) {
                                        with_status(&shared, |status| {
                                            status.last_error =
                                                Some(format!("virtual file stream: {error}"));
                                        });
                                    }
                                    #[cfg(target_os = "linux")]
                                    match current.producer.try_push(data) {
                                        Ok(true) => {}
                                        Ok(false) => {
                                            pending_virtual_chunk =
                                                Some(PendingLinuxVirtualChunk {
                                                    transfer_id: transfer_id.clone(),
                                                    data: data.clone(),
                                                });
                                        }
                                        Err(error) => {
                                            with_status(&shared, |status| {
                                                status.last_error =
                                                    Some(format!("virtual file stream: {error}"));
                                            });
                                            pending_virtual_chunk =
                                                Some(PendingLinuxVirtualChunk {
                                                    transfer_id: transfer_id.clone(),
                                                    data: data.clone(),
                                                });
                                        }
                                    }
                                }
                            }
                        }
                        #[cfg(target_os = "windows")]
                        {
                            match &file_event {
                                InboundFileResult::Offered {
                                    transfer_id,
                                    file_name,
                                    size,
                                } => {
                                    let next = DeferredVirtualOffer {
                                        transfer_id: transfer_id.clone(),
                                        file_name: file_name.clone(),
                                        size: *size,
                                    };
                                    let must_defer =
                                        virtual_receive.as_ref().is_some_and(|current| {
                                            active_virtual_receive_must_finish(
                                                current.requested,
                                                current.completed,
                                            )
                                        }) || virtual_batch_receive
                                            .as_ref()
                                            .is_some_and(WindowsVirtualBatchReceive::must_finish);
                                    if must_defer {
                                        if let Some(stale) = deferred_virtual_batch_offer.take() {
                                            cancel_deferred_windows_virtual_batch(
                                                session,
                                                conn,
                                                stale,
                                                "replaced by a newer deferred file offer",
                                            )?;
                                        }
                                        if let Some(stale) = deferred_virtual_offer.replace(next) {
                                            session
                                                .cancel_file(
                                                    stale.transfer_id,
                                                    "replaced by a newer deferred file offer",
                                                )
                                                .map_err(|e| e.to_string())?;
                                            conn.send_all(session.take_outbox().iter())
                                                .map_err(|e| e.to_string())?;
                                        }
                                    } else {
                                        if let Some(previous) = virtual_receive.take() {
                                            if !previous.completed {
                                                previous
                                                    .producer
                                                    .fail("replaced by a newer file offer");
                                                session
                                                    .cancel_file(
                                                        previous.transfer_id,
                                                        "replaced by a newer file offer",
                                                    )
                                                    .map_err(|e| e.to_string())?;
                                                conn.send_all(session.take_outbox().iter())
                                                    .map_err(|e| e.to_string())?;
                                            }
                                        }
                                        if let Some(previous) = virtual_batch_receive.take() {
                                            cancel_windows_virtual_batch(
                                                session,
                                                conn,
                                                &previous,
                                                "replaced by a newer file offer",
                                            )?;
                                        }
                                        if let Some(stale) = deferred_virtual_offer.take() {
                                            session
                                                .cancel_file(
                                                    stale.transfer_id,
                                                    "replaced by a newer file offer",
                                                )
                                                .map_err(|e| e.to_string())?;
                                            conn.send_all(session.take_outbox().iter())
                                                .map_err(|e| e.to_string())?;
                                        }
                                        if let Some(stale) = deferred_virtual_batch_offer.take() {
                                            cancel_deferred_windows_virtual_batch(
                                                session,
                                                conn,
                                                stale,
                                                "replaced by a newer file offer",
                                            )?;
                                        }
                                        virtual_receive = Some(publish_windows_virtual_offer(
                                            &ole_manager,
                                            &next,
                                        )?);
                                        mark_virtual_offer(&shared, &next);
                                    }
                                }
                                InboundFileResult::Failed {
                                    transfer_id,
                                    message,
                                } => {
                                    if deferred_virtual_offer
                                        .as_ref()
                                        .is_some_and(|v| v.transfer_id == *transfer_id)
                                    {
                                        deferred_virtual_offer = None;
                                    }
                                    if virtual_receive
                                        .as_ref()
                                        .is_some_and(|v| v.transfer_id == *transfer_id)
                                    {
                                        if let Some(current) = virtual_receive.take() {
                                            task_057_diagnostic(format_args!(
                                                "single_network_stream_failed entry_id={transfer_id:?} path={:?} message={message:?} since_publish_ms={}",
                                                current.file_name,
                                                Instant::now()
                                                    .saturating_duration_since(current.published_at)
                                                    .as_millis()
                                            ));
                                            current.producer.fail(message.clone());
                                        }
                                        ole_manager.clear();
                                    }
                                    with_status(&shared, |s| {
                                        mark_file_failed_if_current(s, transfer_id, message);
                                    });
                                }
                                InboundFileResult::StreamCompleted {
                                    transfer_id, size, ..
                                } => {
                                    if let Some(current) = virtual_receive
                                        .as_mut()
                                        .filter(|current| current.transfer_id == *transfer_id)
                                    {
                                        let completed_at = Instant::now();
                                        let network_elapsed =
                                            current.network_started_at.map(|started| {
                                                completed_at.saturating_duration_since(started)
                                            });
                                        let data_elapsed = current.first_chunk_at.map(|started| {
                                            completed_at.saturating_duration_since(started)
                                        });
                                        let effective_mib_per_second = network_elapsed
                                            .filter(|elapsed| !elapsed.is_zero())
                                            .map(|elapsed| {
                                                *size as f64 / 1_048_576.0 / elapsed.as_secs_f64()
                                            })
                                            .unwrap_or(0.0);
                                        let data_mib_per_second = data_elapsed
                                            .filter(|elapsed| !elapsed.is_zero())
                                            .map(|elapsed| {
                                                *size as f64 / 1_048_576.0 / elapsed.as_secs_f64()
                                            })
                                            .unwrap_or(0.0);
                                        task_057_diagnostic(format_args!(
                                            "single_network_stream_completed entry_id={transfer_id:?} path={:?} size={size} network_ms={} data_ms={} effective_mib_s={effective_mib_per_second:.2} data_mib_s={data_mib_per_second:.2} since_publish_ms={}",
                                            current.file_name,
                                            network_elapsed
                                                .map_or(0, |elapsed| elapsed.as_millis()),
                                            data_elapsed.map_or(0, |elapsed| elapsed.as_millis()),
                                            completed_at
                                                .saturating_duration_since(current.published_at)
                                                .as_millis()
                                        ));
                                        current.producer.finish();
                                        current.completed = true;
                                        with_status(&shared, |s| {
                                            s.file_transfer_phase = Some("done".into());
                                            s.last_file_transfer_id = Some(transfer_id.clone());
                                            s.file_bytes_received = Some(*size);
                                            s.file_bytes_total = Some(*size);
                                            s.last_error = None;
                                        });
                                    }
                                }
                                InboundFileResult::Chunk { transfer_id, data }
                                    if virtual_receive.as_ref().is_some_and(|current| {
                                        current.transfer_id == *transfer_id
                                    }) =>
                                {
                                    if let Some((_, received, total)) =
                                        session.inbound_file_progress()
                                    {
                                        with_status(&shared, |s| {
                                            s.file_transfer_phase = Some("receiving".into());
                                            s.last_file_transfer_id = Some(transfer_id.clone());
                                            s.file_bytes_received = Some(received);
                                            s.file_bytes_total = Some(total);
                                            s.last_file_bytes = Some(total);
                                            s.last_error = None;
                                        });
                                    } else {
                                        with_status(&shared, |s| {
                                            s.file_bytes_received = Some(
                                                s.file_bytes_received
                                                    .unwrap_or(0)
                                                    .saturating_add(data.len() as u64),
                                            );
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        #[cfg(target_os = "linux")]
                        {
                            match &file_event {
                                InboundFileResult::Offered {
                                    transfer_id,
                                    file_name,
                                    size,
                                } => {
                                    let next = DeferredVirtualOffer {
                                        transfer_id: transfer_id.clone(),
                                        file_name: file_name.clone(),
                                        size: *size,
                                    };
                                    let must_defer =
                                        virtual_receive.as_ref().is_some_and(|current| {
                                            linux_virtual_receive_must_finish(
                                                current.requested,
                                                current.completed,
                                                current.consumed,
                                                current.released,
                                            )
                                        }) || virtual_batch_receive
                                            .as_ref()
                                            .is_some_and(LinuxVirtualBatchReceive::must_finish);
                                    if must_defer {
                                        if let Some(stale) = deferred_virtual_batch_offer.take() {
                                            cancel_deferred_linux_virtual_batch(
                                                session,
                                                conn,
                                                stale,
                                                "replaced by a newer deferred file offer",
                                            )?;
                                        }
                                        if let Some(stale) = deferred_virtual_offer.replace(next) {
                                            session
                                                .cancel_file(
                                                    stale.transfer_id,
                                                    "replaced by a newer deferred file offer",
                                                )
                                                .map_err(|e| e.to_string())?;
                                            conn.send_all(session.take_outbox().iter())
                                                .map_err(|e| e.to_string())?;
                                        }
                                    } else {
                                        if let Some(previous) = virtual_receive.take() {
                                            if !previous.completed {
                                                previous
                                                    .producer
                                                    .fail("replaced by a newer file offer");
                                                session
                                                    .cancel_file(
                                                        previous.transfer_id,
                                                        "replaced by a newer file offer",
                                                    )
                                                    .map_err(|e| e.to_string())?;
                                                conn.send_all(session.take_outbox().iter())
                                                    .map_err(|e| e.to_string())?;
                                            }
                                        }
                                        if let Some(previous) = virtual_batch_receive.take() {
                                            cancel_linux_virtual_batch(
                                                session,
                                                conn,
                                                &previous,
                                                "replaced by a newer file offer",
                                            )?;
                                        }
                                        if let Some(stale) = deferred_virtual_offer.take() {
                                            session
                                                .cancel_file(
                                                    stale.transfer_id,
                                                    "replaced by a newer file offer",
                                                )
                                                .map_err(|e| e.to_string())?;
                                            conn.send_all(session.take_outbox().iter())
                                                .map_err(|e| e.to_string())?;
                                        }
                                        if let Some(stale) = deferred_virtual_batch_offer.take() {
                                            cancel_deferred_linux_virtual_batch(
                                                session,
                                                conn,
                                                stale,
                                                "replaced by a newer file offer",
                                            )?;
                                        }
                                        let published = clipboard
                                            .as_mut()
                                            .ok_or_else(|| {
                                                "Linux clipboard unavailable for virtual file"
                                                    .to_string()
                                            })
                                            .and_then(|clip| {
                                                publish_linux_virtual_offer(
                                                    &mut fuse_manager,
                                                    clip,
                                                    &next,
                                                )
                                            });
                                        match published {
                                            Ok(receive) => {
                                                virtual_receive = Some(receive);
                                                mark_virtual_offer(&shared, &next);
                                            }
                                            Err(error) => {
                                                fuse_manager.clear();
                                                session
                                                    .cancel_file(
                                                        next.transfer_id.clone(),
                                                        format!("FUSE publish failed: {error}"),
                                                    )
                                                    .map_err(|e| e.to_string())?;
                                                conn.send_all(session.take_outbox().iter())
                                                    .map_err(|e| e.to_string())?;
                                                with_status(&shared, |s| {
                                                    s.file_transfer_phase = Some("failed".into());
                                                    s.last_error =
                                                        Some(format!("FUSE publish: {error}"));
                                                });
                                            }
                                        }
                                    }
                                }
                                InboundFileResult::Failed {
                                    transfer_id,
                                    message,
                                } => {
                                    if deferred_virtual_offer
                                        .as_ref()
                                        .is_some_and(|v| v.transfer_id == *transfer_id)
                                    {
                                        deferred_virtual_offer = None;
                                    }
                                    if virtual_receive
                                        .as_ref()
                                        .is_some_and(|v| v.transfer_id == *transfer_id)
                                    {
                                        if let Some(current) = virtual_receive.take() {
                                            current.producer.fail(message.clone());
                                        }
                                        fuse_manager.clear();
                                    }
                                    with_status(&shared, |s| {
                                        mark_file_failed_if_current(s, transfer_id, message);
                                    });
                                }
                                InboundFileResult::StreamCompleted {
                                    transfer_id, size, ..
                                } => {
                                    if let Some(current) = virtual_receive
                                        .as_mut()
                                        .filter(|current| current.transfer_id == *transfer_id)
                                    {
                                        current.producer.finish();
                                        current.completed = true;
                                        with_status(&shared, |s| {
                                            s.file_transfer_phase = Some("done".into());
                                            s.last_file_transfer_id = Some(transfer_id.clone());
                                            s.file_bytes_received = Some(*size);
                                            s.file_bytes_total = Some(*size);
                                            s.last_error = None;
                                        });
                                    }
                                }
                                InboundFileResult::Chunk { transfer_id, data }
                                    if virtual_receive.as_ref().is_some_and(|current| {
                                        current.transfer_id == *transfer_id
                                    }) =>
                                {
                                    if let Some((_, received, total)) =
                                        session.inbound_file_progress()
                                    {
                                        with_status(&shared, |s| {
                                            s.file_transfer_phase = Some("receiving".into());
                                            s.last_file_transfer_id = Some(transfer_id.clone());
                                            s.file_bytes_received = Some(received);
                                            s.file_bytes_total = Some(total);
                                            s.last_file_bytes = Some(total);
                                            s.last_error = None;
                                        });
                                    } else {
                                        with_status(&shared, |s| {
                                            s.file_bytes_received = Some(
                                                s.file_bytes_received
                                                    .unwrap_or(0)
                                                    .saturating_add(data.len() as u64),
                                            );
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                        handle_inbound_file(&shared, session, conn, file_event)?;
                        if let Some(transfer_id) = failed_transfer_id {
                            discard_queued_file_status(&mut queued_file_status, &transfer_id);
                            promote_queued_file_status_after(
                                &shared,
                                &mut queued_file_status,
                                &transfer_id,
                            );
                        }
                        if let Some(transfer_id) = cancelled_clipboard_offer {
                            rearm_cancelled_clipboard_offer(
                                clipboard.as_mut(),
                                &mut latest_clipboard_file_offer_id,
                                &transfer_id,
                            );
                        }
                    } else {
                        #[cfg(target_os = "windows")]
                        if let Some(current) = virtual_receive
                            .as_ref()
                            .filter(|current| current.requested && !current.completed)
                        {
                            if let Some((tid, got, total)) = session.inbound_file_progress() {
                                if tid == current.transfer_id {
                                    with_status(&shared, |s| {
                                        s.file_transfer_phase = Some("receiving".into());
                                        s.last_file_transfer_id = Some(tid);
                                        s.file_bytes_received = Some(got);
                                        s.file_bytes_total = Some(total);
                                    });
                                }
                            }
                        }
                        #[cfg(target_os = "windows")]
                        if let Some(batch) = virtual_batch_receive.as_ref() {
                            if let Some(index) = batch.active_index {
                                if let Some((tid, got, total)) = session.inbound_file_progress() {
                                    if tid == batch.files[index].entry.entry_id {
                                        with_status(&shared, |status| {
                                            status.file_transfer_phase = Some("receiving".into());
                                            status.last_file_transfer_id =
                                                Some(batch.batch_id.clone());
                                            status.file_batch_current_path = Some(
                                                batch.files[index].entry.relative_path.clone(),
                                            );
                                            status.file_bytes_received = Some(got);
                                            status.file_bytes_total = Some(total);
                                        });
                                    }
                                }
                            }
                        }
                        #[cfg(target_os = "linux")]
                        if let Some(batch) = virtual_batch_receive.as_ref() {
                            if let Some(index) = batch.active_index {
                                if let Some((tid, got, total)) = session.inbound_file_progress() {
                                    if tid == batch.files[index].entry.entry_id {
                                        with_status(&shared, |status| {
                                            status.file_transfer_phase = Some("receiving".into());
                                            status.last_file_transfer_id =
                                                Some(batch.batch_id.clone());
                                            status.file_batch_current_path = Some(
                                                batch.files[index].entry.relative_path.clone(),
                                            );
                                            status.file_bytes_received = Some(got);
                                            status.file_bytes_total = Some(total);
                                        });
                                    }
                                }
                            }
                        }
                        #[cfg(target_os = "linux")]
                        if inbound_batch.is_none() && virtual_batch_receive.is_none() {
                            if let Some((tid, got, total)) = session.inbound_file_progress() {
                                with_status(&shared, |s| {
                                    s.file_transfer_phase = Some("receiving".into());
                                    s.last_file_transfer_id = Some(tid);
                                    s.file_bytes_received = Some(got);
                                    s.file_bytes_total = Some(total);
                                });
                            }
                        }
                        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
                        if inbound_batch.is_none() {
                            if let Some((tid, got, total)) = session.inbound_file_progress() {
                                with_status(&shared, |s| {
                                    s.file_transfer_phase = Some("receiving".into());
                                    s.last_file_transfer_id = Some(tid);
                                    s.file_bytes_received = Some(got);
                                    s.file_bytes_total = Some(total);
                                });
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(m590_net::TcpError::Disconnected) => return Err("peer disconnected".into()),
                Err(err) => return Err(err.to_string()),
            }
        }

        #[cfg(target_os = "windows")]
        {
            let mut promote_deferred = false;
            let mut promote_deferred_if_current = false;
            let mut discard_deferred = false;
            if let Some(mut current) = virtual_receive.take() {
                let mut keep_current = true;
                while let Some(event) = current.bridge.take_event() {
                    if !keep_current {
                        break;
                    }
                    match event {
                        BridgeEvent::Request => {
                            let requested_at = Instant::now();
                            current.requested = true;
                            current.requested_at.get_or_insert(requested_at);
                            task_057_diagnostic(format_args!(
                                "single_ole_stream_request entry_id={:?} path={:?} size={} since_publish_ms={}",
                                current.transfer_id,
                                current.file_name,
                                current.size,
                                requested_at
                                    .saturating_duration_since(current.published_at)
                                    .as_millis()
                            ));
                            if current.completed {
                                current.producer.finish();
                                continue;
                            }
                            let dispatch_started = Instant::now();
                            match session.request_file_stream(current.transfer_id.clone()) {
                                Ok(QueueFileResult::Queued) => {
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|e| e.to_string())?;
                                    let network_started = Instant::now();
                                    current.producer.start();
                                    current.network_started_at = Some(network_started);
                                    task_057_diagnostic(format_args!(
                                        "single_network_request_sent entry_id={:?} path={:?} request_wait_ms={} dispatch_ms={}",
                                        current.transfer_id,
                                        current.file_name,
                                        current
                                            .requested_at
                                            .map(|requested| dispatch_started
                                                .saturating_duration_since(requested)
                                                .as_millis())
                                            .unwrap_or(0),
                                        network_started
                                            .saturating_duration_since(dispatch_started)
                                            .as_millis()
                                    ));
                                    with_status(&shared, |s| {
                                        s.file_transfer_phase = Some("receiving".into())
                                    });
                                }
                                Ok(other) => {
                                    current.producer.fail(format!("request failed: {other:?}"))
                                }
                                Err(err) => current.producer.fail(err.to_string()),
                            }
                        }
                        BridgeEvent::Consumed => {
                            task_057_diagnostic(format_args!(
                                "single_ole_stream_consumed entry_id={:?} path={:?} since_publish_ms={}",
                                current.transfer_id,
                                current.file_name,
                                Instant::now()
                                    .saturating_duration_since(current.published_at)
                                    .as_millis()
                            ));
                        }
                        BridgeEvent::Released => {}
                        BridgeEvent::Cancel(reason) => {
                            task_057_diagnostic(format_args!(
                                "single_ole_stream_cancel entry_id={:?} path={:?} reason={reason:?}",
                                current.transfer_id,
                                current.file_name
                            ));
                            session
                                .cancel_file(current.transfer_id.clone(), reason)
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                            ole_manager.clear();
                            keep_current = false;
                            promote_deferred = true;
                            break;
                        }
                    }
                }
                while keep_current {
                    if let Some(msg) = ole_manager.take_event() {
                        match msg {
                            ManagerEvent::PublishFailed(error) => {
                                current.producer.fail(error.clone());
                                if !current.completed {
                                    session
                                        .cancel_file(
                                            current.transfer_id.clone(),
                                            format!("OLE publish failed: {error}"),
                                        )
                                        .map_err(|e| e.to_string())?;
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|e| e.to_string())?;
                                }
                                with_status(&shared, |s| {
                                    s.file_transfer_phase = Some("failed".into());
                                    s.last_error = Some(format!("OLE publish: {error}"));
                                });
                                keep_current = false;
                                promote_deferred = true;
                            }
                            ManagerEvent::ClipboardReplaced => {
                                match clipboard_replacement_disposition(
                                    current.requested,
                                    current.completed,
                                ) {
                                    ClipboardReplacementDisposition::KeepActiveTransfer => {
                                        current.clipboard_replaced = true;
                                    }
                                    ClipboardReplacementDisposition::ReleaseClipboardOffer => {
                                        if !current.completed {
                                            current.producer.fail("clipboard replaced");
                                            let transfer_id = current.transfer_id.clone();
                                            session
                                                .cancel_file(transfer_id, "clipboard replaced")
                                                .map_err(|e| e.to_string())?;
                                            conn.send_all(session.take_outbox().iter())
                                                .map_err(|e| e.to_string())?;
                                        }
                                        keep_current = false;
                                    }
                                }
                                discard_deferred = true;
                                latest_clipboard_file_offer_id = None;
                            }
                        }
                    } else {
                        break;
                    }
                }
                if keep_current
                    && completed_replaced_virtual_receive_can_detach(
                        current.completed,
                        current.clipboard_replaced,
                    )
                {
                    keep_current = false;
                } else if keep_current
                    && current.completed
                    && (deferred_virtual_offer.is_some() || deferred_virtual_batch_offer.is_some())
                {
                    keep_current = false;
                    promote_deferred_if_current = true;
                }
                if keep_current {
                    virtual_receive = Some(current);
                }
            }
            if let Some(mut current) = virtual_batch_receive.take() {
                let mut keep_current = true;
                let mut cancel_reason = None;
                for index in 0..current.files.len() {
                    while let Some(event) = current.files[index].bridge.take_event() {
                        match event {
                            BridgeEvent::Request => {
                                let now = Instant::now();
                                let duplicate = current.files[index].requested;
                                current.files[index].requested = true;
                                current.files[index].requested_at.get_or_insert(now);
                                current.clipboard_replaced_idle_since = None;
                                task_057_diagnostic(format_args!(
                                    "ole_stream_request batch_id={:?} lindex={} entry_id={:?} path={:?} duplicate={} since_publish_ms={}",
                                    current.batch_id,
                                    current.files[index].descriptor_index,
                                    current.files[index].entry.entry_id,
                                    current.files[index].entry.relative_path,
                                    duplicate,
                                    now.saturating_duration_since(current.published_at).as_millis()
                                ));
                                if current.files[index].completed {
                                    current.files[index].producer.finish();
                                }
                            }
                            BridgeEvent::Consumed => {
                                task_057_diagnostic(format_args!(
                                    "ole_stream_consumed batch_id={:?} lindex={} entry_id={:?} path={:?} since_publish_ms={}",
                                    current.batch_id,
                                    current.files[index].descriptor_index,
                                    current.files[index].entry.entry_id,
                                    current.files[index].entry.relative_path,
                                    Instant::now()
                                        .saturating_duration_since(current.published_at)
                                        .as_millis()
                                ));
                            }
                            BridgeEvent::Released => {}
                            BridgeEvent::Cancel(reason) => {
                                task_057_diagnostic(format_args!(
                                    "ole_stream_cancel batch_id={:?} lindex={} entry_id={:?} path={:?} reason={reason:?}",
                                    current.batch_id,
                                    current.files[index].descriptor_index,
                                    current.files[index].entry.entry_id,
                                    current.files[index].entry.relative_path
                                ));
                                cancel_reason = Some(reason);
                                break;
                            }
                        }
                    }
                    if cancel_reason.is_some() {
                        break;
                    }
                }

                if let Some(reason) = cancel_reason {
                    cancel_windows_virtual_batch(session, conn, &current, &reason)?;
                    ole_manager.clear();
                    with_status(&shared, |status| {
                        mark_batch_failed(status, &current.batch_id, &reason);
                    });
                    keep_current = false;
                    promote_deferred = true;
                }

                if keep_current && current.active_index.is_none() {
                    if let Some(index) = current.next_requested_index() {
                        let transfer_id = current.files[index].entry.entry_id.clone();
                        let dispatch_started = Instant::now();
                        let request_wait_ms = current.files[index]
                            .requested_at
                            .map(|requested| {
                                dispatch_started
                                    .saturating_duration_since(requested)
                                    .as_millis()
                            })
                            .unwrap_or(0);
                        task_057_diagnostic(format_args!(
                            "network_request_dispatch batch_id={:?} lindex={} entry_id={transfer_id:?} path={:?} request_wait_ms={request_wait_ms}",
                            current.batch_id,
                            current.files[index].descriptor_index,
                            current.files[index].entry.relative_path
                        ));
                        let request_error = match session.request_file_stream(transfer_id) {
                            Ok(QueueFileResult::Queued) => {
                                conn.send_all(session.take_outbox().iter())
                                    .map_err(|error| error.to_string())?;
                                let network_started = Instant::now();
                                current.files[index].producer.start();
                                current.files[index].network_started_at = Some(network_started);
                                current.active_index = Some(index);
                                task_057_diagnostic(format_args!(
                                    "network_request_sent batch_id={:?} lindex={} entry_id={:?} path={:?} dispatch_ms={}",
                                    current.batch_id,
                                    current.files[index].descriptor_index,
                                    current.files[index].entry.entry_id,
                                    current.files[index].entry.relative_path,
                                    network_started
                                        .saturating_duration_since(dispatch_started)
                                        .as_millis()
                                ));
                                with_status(&shared, |status| {
                                    status.file_transfer_phase = Some("receiving".into());
                                    status.last_file_transfer_id = Some(current.batch_id.clone());
                                    status.file_batch_current_path =
                                        Some(current.files[index].entry.relative_path.clone());
                                    status.file_bytes_received = Some(0);
                                    status.file_bytes_total = Some(current.files[index].entry.size);
                                    status.last_error = None;
                                });
                                None
                            }
                            Ok(other) => Some(format!("request failed: {other:?}")),
                            Err(error) => Some(error.to_string()),
                        };
                        if let Some(error) = request_error {
                            cancel_windows_virtual_batch(session, conn, &current, &error)?;
                            ole_manager.clear();
                            with_status(&shared, |status| {
                                mark_batch_failed(status, &current.batch_id, &error);
                            });
                            keep_current = false;
                            promote_deferred = true;
                        }
                    }
                }

                while keep_current {
                    let Some(event) = ole_manager.take_event() else {
                        break;
                    };
                    match event {
                        ManagerEvent::PublishFailed(error) => {
                            cancel_windows_virtual_batch(
                                session,
                                conn,
                                &current,
                                &format!("OLE publish failed: {error}"),
                            )?;
                            with_status(&shared, |status| {
                                mark_batch_failed(status, &current.batch_id, &error);
                            });
                            keep_current = false;
                            promote_deferred = true;
                        }
                        ManagerEvent::ClipboardReplaced => {
                            if current.must_finish() {
                                current.clipboard_replaced = true;
                                current.clipboard_replaced_idle_since = None;
                            } else {
                                cancel_windows_virtual_batch(
                                    session,
                                    conn,
                                    &current,
                                    "clipboard replaced",
                                )?;
                                with_status(&shared, |status| {
                                    status.file_transfer_phase = Some("cancelled".into());
                                    status.file_batch_current_path = None;
                                    status.file_bytes_received = None;
                                    status.file_bytes_total = None;
                                    status.last_error = None;
                                });
                                keep_current = false;
                            }
                            discard_deferred = true;
                            latest_clipboard_file_offer_id = None;
                        }
                    }
                }

                if keep_current && current.clipboard_replaced {
                    if current.is_complete() {
                        keep_current = false;
                    } else if current.must_finish() {
                        current.clipboard_replaced_idle_since = None;
                    } else {
                        let idle_since = current
                            .clipboard_replaced_idle_since
                            .get_or_insert_with(Instant::now);
                        if idle_since.elapsed() >= REPLACED_BATCH_REQUEST_GRACE {
                            cancel_windows_virtual_batch(
                                session,
                                conn,
                                &current,
                                "clipboard replaced",
                            )?;
                            with_status(&shared, |status| {
                                status.file_transfer_phase = Some("cancelled".into());
                                status.file_batch_current_path = None;
                                status.file_bytes_received = None;
                                status.file_bytes_total = None;
                                status.last_error = None;
                            });
                            keep_current = false;
                        }
                    }
                }
                if keep_current
                    && !current.must_finish()
                    && (deferred_virtual_offer.is_some() || deferred_virtual_batch_offer.is_some())
                {
                    cancel_windows_virtual_batch(
                        session,
                        conn,
                        &current,
                        "replaced after active batch streams completed",
                    )?;
                    keep_current = false;
                    promote_deferred_if_current = true;
                }
                if keep_current {
                    virtual_batch_receive = Some(current);
                }
            }
            if discard_deferred {
                if let Some(stale) = deferred_virtual_offer.take() {
                    session
                        .cancel_file(stale.transfer_id, "clipboard replaced")
                        .map_err(|e| e.to_string())?;
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                }
                if let Some(stale) = deferred_virtual_batch_offer.take() {
                    cancel_deferred_windows_virtual_batch(
                        session,
                        conn,
                        stale,
                        "clipboard replaced",
                    )?;
                }
            } else if promote_deferred {
                if let Some(next) = deferred_virtual_offer.take() {
                    virtual_receive = Some(publish_windows_virtual_offer(&ole_manager, &next)?);
                    mark_virtual_offer(&shared, &next);
                } else if let Some(next) = deferred_virtual_batch_offer.take() {
                    virtual_batch_receive =
                        Some(publish_windows_virtual_batch_offer(&ole_manager, &next)?);
                    mark_windows_virtual_batch_offer(&shared, &next);
                }
            } else if promote_deferred_if_current {
                if let Some(next) = deferred_virtual_offer.take() {
                    match replace_windows_virtual_offer_if_current(&ole_manager, &next)? {
                        Some(receive) => {
                            virtual_receive = Some(receive);
                            mark_virtual_offer(&shared, &next);
                        }
                        None => {
                            session
                                .cancel_file(next.transfer_id, "clipboard replaced")
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                            latest_clipboard_file_offer_id = None;
                            while ole_manager.take_event().is_some() {}
                        }
                    }
                } else if let Some(next) = deferred_virtual_batch_offer.take() {
                    match replace_windows_virtual_batch_offer_if_current(&ole_manager, &next)? {
                        Some(receive) => {
                            virtual_batch_receive = Some(receive);
                            mark_windows_virtual_batch_offer(&shared, &next);
                        }
                        None => {
                            cancel_deferred_windows_virtual_batch(
                                session,
                                conn,
                                next,
                                "clipboard replaced",
                            )?;
                            latest_clipboard_file_offer_id = None;
                            while ole_manager.take_event().is_some() {}
                        }
                    }
                }
            }
            if virtual_receive.is_none() && virtual_batch_receive.is_none() {
                if let Some(next) = deferred_virtual_offer.take() {
                    virtual_receive = Some(publish_windows_virtual_offer(&ole_manager, &next)?);
                    mark_virtual_offer(&shared, &next);
                } else if let Some(next) = deferred_virtual_batch_offer.take() {
                    virtual_batch_receive =
                        Some(publish_windows_virtual_batch_offer(&ole_manager, &next)?);
                    mark_windows_virtual_batch_offer(&shared, &next);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            let mut promote_deferred = false;
            let mut promote_deferred_if_current = false;
            let mut discard_deferred = false;
            if let Some(mut current) = virtual_receive.take() {
                let mut keep_current = true;
                while let Some(event) = current.bridge.take_event() {
                    if !keep_current {
                        break;
                    }
                    match event {
                        BridgeEvent::Request => {
                            current.requested = true;
                            if current.completed {
                                current.producer.finish();
                                continue;
                            }
                            match session.request_file_stream(current.transfer_id.clone()) {
                                Ok(QueueFileResult::Queued) => {
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|e| e.to_string())?;
                                    current.producer.start();
                                    with_status(&shared, |s| {
                                        s.file_transfer_phase = Some("receiving".into())
                                    });
                                }
                                Ok(other) => {
                                    current.producer.fail(format!("request failed: {other:?}"))
                                }
                                Err(err) => current.producer.fail(err.to_string()),
                            }
                        }
                        BridgeEvent::Consumed => current.consumed = true,
                        BridgeEvent::Released => current.released = true,
                        BridgeEvent::Cancel(reason) => {
                            session
                                .cancel_file(current.transfer_id.clone(), reason.clone())
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                            with_status(&shared, |s| {
                                mark_file_failed_if_current(s, &current.transfer_id, &reason);
                            });
                            fuse_manager.clear();
                            keep_current = false;
                            promote_deferred = true;
                            break;
                        }
                    }
                }

                if keep_current && !current.clipboard_replaced {
                    if let Some(clip) = clipboard.as_mut() {
                        match fuse_manager.is_current(clip) {
                            Ok(true) => {}
                            Ok(false) => {
                                if linux_virtual_receive_must_finish(
                                    current.requested,
                                    current.completed,
                                    current.consumed,
                                    current.released,
                                ) {
                                    current.clipboard_replaced = true;
                                } else {
                                    if !current.completed {
                                        current.producer.fail("clipboard replaced");
                                        session
                                            .cancel_file(
                                                current.transfer_id.clone(),
                                                "clipboard replaced",
                                            )
                                            .map_err(|e| e.to_string())?;
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|e| e.to_string())?;
                                    }
                                    fuse_manager.clear();
                                    keep_current = false;
                                }
                                discard_deferred = true;
                                latest_clipboard_file_offer_id = None;
                            }
                            Err(error) => {
                                with_status(&shared, |s| {
                                    s.last_error =
                                        Some(format!("FUSE clipboard ownership: {error}"));
                                });
                            }
                        }
                    }
                }

                if keep_current
                    && linux_completed_replaced_virtual_receive_can_detach(
                        current.completed,
                        current.consumed,
                        current.released,
                        current.clipboard_replaced,
                    )
                {
                    fuse_manager.clear();
                    keep_current = false;
                } else if keep_current
                    && current.completed
                    && current.consumed
                    && current.released
                    && (deferred_virtual_offer.is_some() || deferred_virtual_batch_offer.is_some())
                {
                    keep_current = false;
                    promote_deferred_if_current = true;
                }
                if keep_current {
                    virtual_receive = Some(current);
                }
            }

            if let Some(mut current) = virtual_batch_receive.take() {
                let mut keep_current = true;
                let mut cancel_reason = None;
                for index in 0..current.files.len() {
                    while let Some(event) = current.files[index].bridge.take_event() {
                        match event {
                            BridgeEvent::Request => {
                                current.files[index].requested = true;
                                if current.files[index].completed {
                                    current.files[index].producer.finish();
                                }
                            }
                            BridgeEvent::Consumed => current.files[index].consumed = true,
                            BridgeEvent::Released => current.files[index].released = true,
                            BridgeEvent::Cancel(reason) => {
                                cancel_reason = Some(reason);
                                break;
                            }
                        }
                    }
                    if cancel_reason.is_some() {
                        break;
                    }
                }

                if let Some(reason) = cancel_reason {
                    cancel_linux_virtual_batch(session, conn, &current, &reason)?;
                    fuse_manager.clear();
                    with_status(&shared, |status| {
                        mark_batch_failed(status, &current.batch_id, &reason);
                    });
                    keep_current = false;
                    promote_deferred = true;
                }

                if keep_current && current.active_index.is_none() {
                    if let Some(index) = current.next_requested_index() {
                        let transfer_id = current.files[index].entry.entry_id.clone();
                        let request_error = match session.request_file_stream(transfer_id) {
                            Ok(QueueFileResult::Queued) => {
                                conn.send_all(session.take_outbox().iter())
                                    .map_err(|error| error.to_string())?;
                                current.files[index].producer.start();
                                current.active_index = Some(index);
                                with_status(&shared, |status| {
                                    status.file_transfer_phase = Some("receiving".into());
                                    status.last_file_transfer_id = Some(current.batch_id.clone());
                                    status.file_batch_current_path =
                                        Some(current.files[index].entry.relative_path.clone());
                                    status.file_bytes_received = Some(0);
                                    status.file_bytes_total = Some(current.files[index].entry.size);
                                    status.last_error = None;
                                });
                                None
                            }
                            Ok(other) => Some(format!("request failed: {other:?}")),
                            Err(error) => Some(error.to_string()),
                        };
                        if let Some(error) = request_error {
                            cancel_linux_virtual_batch(session, conn, &current, &error)?;
                            fuse_manager.clear();
                            with_status(&shared, |status| {
                                mark_batch_failed(status, &current.batch_id, &error);
                            });
                            keep_current = false;
                            promote_deferred = true;
                        }
                    }
                }

                if keep_current && !current.clipboard_replaced {
                    if let Some(clip) = clipboard.as_mut() {
                        match fuse_manager.is_current(clip) {
                            Ok(true) => {}
                            Ok(false) => {
                                if current.must_finish() {
                                    current.clipboard_replaced = true;
                                } else {
                                    cancel_linux_virtual_batch(
                                        session,
                                        conn,
                                        &current,
                                        "clipboard replaced",
                                    )?;
                                    fuse_manager.clear();
                                    with_status(&shared, |status| {
                                        status.file_transfer_phase = Some("cancelled".into());
                                        status.file_batch_current_path = None;
                                        status.file_bytes_received = None;
                                        status.file_bytes_total = None;
                                        status.last_error = None;
                                    });
                                    keep_current = false;
                                }
                                discard_deferred = true;
                                latest_clipboard_file_offer_id = None;
                            }
                            Err(error) => {
                                with_status(&shared, |status| {
                                    status.last_error =
                                        Some(format!("FUSE clipboard ownership: {error}"));
                                });
                            }
                        }
                    }
                }

                if keep_current && current.clipboard_replaced && !current.must_finish() {
                    cancel_linux_virtual_batch(session, conn, &current, "clipboard replaced")?;
                    fuse_manager.clear();
                    keep_current = false;
                } else if keep_current
                    && !current.must_finish()
                    && (deferred_virtual_offer.is_some() || deferred_virtual_batch_offer.is_some())
                {
                    cancel_linux_virtual_batch(
                        session,
                        conn,
                        &current,
                        "replaced after active batch streams completed",
                    )?;
                    keep_current = false;
                    promote_deferred_if_current = true;
                }
                if keep_current {
                    virtual_batch_receive = Some(current);
                }
            }

            if discard_deferred {
                if let Some(stale) = deferred_virtual_offer.take() {
                    session
                        .cancel_file(stale.transfer_id, "clipboard replaced")
                        .map_err(|e| e.to_string())?;
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                }
                if let Some(stale) = deferred_virtual_batch_offer.take() {
                    cancel_deferred_linux_virtual_batch(
                        session,
                        conn,
                        stale,
                        "clipboard replaced",
                    )?;
                }
            } else if promote_deferred {
                if let Some(next) = deferred_virtual_offer.take() {
                    let published = clipboard
                        .as_mut()
                        .ok_or_else(|| "Linux clipboard unavailable for virtual file".to_string())
                        .and_then(|clip| {
                            publish_linux_virtual_offer(&mut fuse_manager, clip, &next)
                        });
                    match published {
                        Ok(receive) => {
                            virtual_receive = Some(receive);
                            mark_virtual_offer(&shared, &next);
                        }
                        Err(error) => {
                            fuse_manager.clear();
                            session
                                .cancel_file(
                                    next.transfer_id,
                                    format!("FUSE publish failed: {error}"),
                                )
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                        }
                    }
                } else if let Some(next) = deferred_virtual_batch_offer.take() {
                    let published = clipboard
                        .as_mut()
                        .ok_or_else(|| "Linux clipboard unavailable for virtual batch".to_string())
                        .and_then(|clip| {
                            publish_linux_virtual_batch_offer(&mut fuse_manager, clip, &next)
                        });
                    match published {
                        Ok(receive) => {
                            virtual_batch_receive = Some(receive);
                            mark_linux_virtual_batch_offer(&shared, &next);
                        }
                        Err(error) => {
                            fuse_manager.clear();
                            cancel_deferred_linux_virtual_batch(
                                session,
                                conn,
                                next,
                                &format!("FUSE publish failed: {error}"),
                            )?;
                        }
                    }
                }
            } else if promote_deferred_if_current {
                if let Some(next) = deferred_virtual_offer.take() {
                    let replaced = clipboard
                        .as_mut()
                        .ok_or_else(|| "Linux clipboard unavailable for virtual file".to_string())
                        .and_then(|clip| {
                            replace_linux_virtual_offer_if_current(&mut fuse_manager, clip, &next)
                        });
                    match replaced {
                        Ok(Some(receive)) => {
                            virtual_receive = Some(receive);
                            mark_virtual_offer(&shared, &next);
                        }
                        Ok(None) => {
                            session
                                .cancel_file(next.transfer_id, "clipboard replaced")
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                            latest_clipboard_file_offer_id = None;
                        }
                        Err(error) => {
                            fuse_manager.clear();
                            session
                                .cancel_file(
                                    next.transfer_id,
                                    format!("FUSE replace failed: {error}"),
                                )
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                            with_status(&shared, |s| {
                                s.file_transfer_phase = Some("failed".into());
                                s.last_error = Some(format!("FUSE replace: {error}"));
                            });
                        }
                    }
                } else if let Some(next) = deferred_virtual_batch_offer.take() {
                    let replaced = clipboard
                        .as_mut()
                        .ok_or_else(|| "Linux clipboard unavailable for virtual batch".to_string())
                        .and_then(|clip| {
                            replace_linux_virtual_batch_offer_if_current(
                                &mut fuse_manager,
                                clip,
                                &next,
                            )
                        });
                    match replaced {
                        Ok(Some(receive)) => {
                            virtual_batch_receive = Some(receive);
                            mark_linux_virtual_batch_offer(&shared, &next);
                        }
                        Ok(None) => {
                            cancel_deferred_linux_virtual_batch(
                                session,
                                conn,
                                next,
                                "clipboard replaced",
                            )?;
                            latest_clipboard_file_offer_id = None;
                        }
                        Err(error) => {
                            fuse_manager.clear();
                            cancel_deferred_linux_virtual_batch(
                                session,
                                conn,
                                next,
                                &format!("FUSE replace failed: {error}"),
                            )?;
                            with_status(&shared, |status| {
                                status.file_transfer_phase = Some("failed".into());
                                status.last_error = Some(format!("FUSE replace: {error}"));
                            });
                        }
                    }
                }
            }
            if virtual_receive.is_none() && virtual_batch_receive.is_none() {
                if let Some(next) = deferred_virtual_offer.take() {
                    let published = clipboard
                        .as_mut()
                        .ok_or_else(|| "Linux clipboard unavailable for virtual file".to_string())
                        .and_then(|clip| {
                            publish_linux_virtual_offer(&mut fuse_manager, clip, &next)
                        });
                    match published {
                        Ok(receive) => {
                            virtual_receive = Some(receive);
                            mark_virtual_offer(&shared, &next);
                        }
                        Err(error) => {
                            fuse_manager.clear();
                            session
                                .cancel_file(
                                    next.transfer_id,
                                    format!("FUSE publish failed: {error}"),
                                )
                                .map_err(|e| e.to_string())?;
                            conn.send_all(session.take_outbox().iter())
                                .map_err(|e| e.to_string())?;
                        }
                    }
                } else if let Some(next) = deferred_virtual_batch_offer.take() {
                    let published = clipboard
                        .as_mut()
                        .ok_or_else(|| "Linux clipboard unavailable for virtual batch".to_string())
                        .and_then(|clip| {
                            publish_linux_virtual_batch_offer(&mut fuse_manager, clip, &next)
                        });
                    match published {
                        Ok(receive) => {
                            virtual_batch_receive = Some(receive);
                            mark_linux_virtual_batch_offer(&shared, &next);
                        }
                        Err(error) => {
                            fuse_manager.clear();
                            cancel_deferred_linux_virtual_batch(
                                session,
                                conn,
                                next,
                                &format!("FUSE publish failed: {error}"),
                            )?;
                        }
                    }
                }
            }
        }

        if let Some(clip) = clipboard.as_mut() {
            let auto = with_status(&shared, |s| s.auto_sync);
            #[cfg(target_os = "windows")]
            let virtual_clipboard_active =
                virtual_receive.is_some() || virtual_batch_receive.is_some();
            #[cfg(target_os = "linux")]
            let virtual_clipboard_active =
                virtual_receive.is_some() || virtual_batch_receive.is_some();
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            let virtual_clipboard_active = false;
            if auto && !virtual_clipboard_active {
                // File-manager copies expose text/uri-list (file_list), not plain text/image.
                if let Ok(Some(paths)) = clip.poll_file_list_change() {
                    latest_clipboard_file_offer_id = None;
                    let mut handled = false;
                    let requires_batch = file_list_requires_batch(&paths);
                    let file_count = paths.iter().filter(|path| path.is_file()).count();
                    let directory_count = paths.iter().filter(|path| path.is_dir()).count();
                    task_057_diagnostic(format_args!(
                        "clipboard_file_list_detected roots={} files={file_count} directories={directory_count} action={}",
                        paths.len(),
                        if requires_batch {
                            "batch"
                        } else {
                            "single"
                        }
                    ));

                    // Multiple roots or any directory need a manifest. Keep the existing
                    // bitmap promotion only for one copied image, so mixed/image batches
                    // retain their original file semantics.
                    if requires_batch {
                        // A batch selection must never degrade into a partial single-file
                        // offer when scanning fails. Preserve the error and wait for a new copy.
                        handled = true;
                        let batch_paths = paths
                            .iter()
                            .map(|path| path.to_string_lossy().into_owned())
                            .collect::<Vec<_>>();
                        match push_batch(&shared, batch_paths) {
                            Ok(()) => {
                                task_057_diagnostic(format_args!(
                                    "clipboard_batch_queued roots={} files={file_count} directories={directory_count}",
                                    paths.len()
                                ));
                            }
                            Err(err) => {
                                task_057_diagnostic(format_args!(
                                    "clipboard_batch_rejected roots={} error={err:?}",
                                    paths.len()
                                ));
                                with_status(&shared, |s| {
                                    s.file_transfer_phase = Some("failed".into());
                                    s.last_error = Some(format!("file_list batch: {err}"));
                                });
                            }
                        }
                    } else if let Ok(Some(image)) = m590_clipboard::image_from_paths(&paths) {
                        // One image keeps bitmap clipboard behavior for Word/paint paste.
                        content_seq += 1;
                        let cid = format!("ui-clip-imgfiles-{}-{content_seq}", std::process::id());
                        match image.prepare_inline(m590_core::Session::INLINE_IMAGE_MAX_BYTES) {
                            Ok((encoding, data)) => {
                                let summary = format!(
                                    "[image {}x{} {}B {encoding:?} from file]",
                                    image.width,
                                    image.height,
                                    data.len()
                                );
                                match session.queue_clipboard_image_encoded(
                                    cid,
                                    image.width,
                                    image.height,
                                    encoding,
                                    data,
                                ) {
                                    Ok(QueueClipboardResult::Queued) => {
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|e| e.to_string())?;
                                        with_status(&shared, |s| {
                                            s.last_sync_text = Some(summary);
                                            s.last_error = None;
                                        });
                                        handled = true;
                                    }
                                    Ok(QueueClipboardResult::ImageTooLarge { byte_len, limit }) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!(
                                                "图片过大已跳过 {byte_len}B > {limit}B"
                                            ));
                                        });
                                        handled = true;
                                    }
                                    Ok(_) => {
                                        handled = true;
                                    }
                                    Err(err) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!("queue_image: {err}"));
                                        });
                                        handled = true;
                                    }
                                }
                            }
                            Err(err) => {
                                with_status(&shared, |s| {
                                    s.last_error = Some(format!("prepare_image: {err}"));
                                });
                                handled = true;
                            }
                        }
                    }
                    // One non-image regular file keeps the proven single-file OLE lifecycle.
                    if !handled {
                        if let Some(path) = m590_clipboard::first_regular_file(&paths) {
                            match offer_local_file(
                                session,
                                &mut content_seq,
                                &path.to_string_lossy(),
                            ) {
                                Ok((summary, transfer_id, file_name, bytes)) => {
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|e| e.to_string())?;
                                    latest_clipboard_file_offer_id = Some(transfer_id.clone());
                                    queue_or_mark_file_sending(
                                        &shared,
                                        session.outbound_file_progress().is_some(),
                                        &mut queued_file_status,
                                        QueuedFileStatus {
                                            summary,
                                            transfer_id,
                                            file_name,
                                            bytes,
                                        },
                                    );
                                    handled = true;
                                }
                                Err(err) => {
                                    with_status(&shared, |s| {
                                        s.last_error = Some(format!("file_list skip: {err}"));
                                    });
                                }
                            }
                        }
                    }
                    // File managers may publish both a file list and the same path as text.
                    // Once the file-list representation was handled, adopt the text baseline so
                    // the same copy produces one offer instead of replacing the OLE clipboard.
                    if handled {
                        clip.adopt_text_baseline();
                    }
                }
                if let Ok(Some(text)) = clip.poll_text_change() {
                    latest_clipboard_file_offer_id = None;
                    // File-manager "copy image file" often only places a path/URI as text.
                    // Prefer decoding local image files into ClipboardImage.
                    if let Ok(Some(image)) = m590_clipboard::image_from_clipboard_text(&text) {
                        content_seq += 1;
                        let cid = format!("ui-clip-imgfile-{}-{content_seq}", std::process::id());
                        match image.prepare_inline(m590_core::Session::INLINE_IMAGE_MAX_BYTES) {
                            Ok((encoding, data)) => {
                                let summary = format!(
                                    "[image {}x{} {}B {encoding:?} from path]",
                                    image.width,
                                    image.height,
                                    data.len()
                                );
                                match session.queue_clipboard_image_encoded(
                                    cid,
                                    image.width,
                                    image.height,
                                    encoding,
                                    data,
                                ) {
                                    Ok(QueueClipboardResult::Queued) => {
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|e| e.to_string())?;
                                        with_status(&shared, |s| {
                                            s.last_sync_text = Some(summary);
                                            s.last_error = None;
                                        });
                                    }
                                    Ok(QueueClipboardResult::ImageTooLarge { byte_len, limit }) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!(
                                                "图片过大已跳过 {byte_len}B > {limit}B"
                                            ));
                                        });
                                    }
                                    Ok(_) => {}
                                    Err(err) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!("queue_image: {err}"));
                                        });
                                    }
                                }
                            }
                            Err(err) => {
                                with_status(&shared, |s| {
                                    s.last_error = Some(format!("prepare_image: {err}"));
                                });
                            }
                        }
                    } else {
                        // GNOME/Nautilus can expose a copied selection as a multi-line
                        // text/uri-list or x-special/gnome-copied-files payload. If the
                        // platform file-list API returned no complete list, recover every
                        // existing path before falling back to the single-file offer.
                        let text_paths = m590_clipboard::local_paths_from_text(&text);
                        let mut handled_text_path = false;
                        if file_list_requires_batch(&text_paths) {
                            let file_count =
                                text_paths.iter().filter(|path| path.is_file()).count();
                            let directory_count =
                                text_paths.iter().filter(|path| path.is_dir()).count();
                            task_057_diagnostic(format_args!(
                                "clipboard_text_paths_detected roots={} files={file_count} directories={directory_count} action=batch",
                                text_paths.len()
                            ));
                            let batch_paths = text_paths
                                .iter()
                                .map(|path| path.to_string_lossy().into_owned())
                                .collect::<Vec<_>>();
                            match push_batch(&shared, batch_paths) {
                                Ok(()) => {
                                    handled_text_path = true;
                                    task_057_diagnostic(format_args!(
                                        "clipboard_batch_queued_from_text roots={} files={file_count} directories={directory_count}",
                                        text_paths.len()
                                    ));
                                }
                                Err(err) => {
                                    with_status(&shared, |s| {
                                        s.file_transfer_phase = Some("failed".into());
                                        s.last_error = Some(format!("path text batch: {err}"));
                                    });
                                }
                            }
                        }
                        if !handled_text_path {
                            if let Some(path) = m590_clipboard::regular_file_from_text(&text) {
                                // Linux often exposes a copied file as plain path text only.
                                match offer_local_file(
                                    session,
                                    &mut content_seq,
                                    &path.to_string_lossy(),
                                ) {
                                    Ok((summary, transfer_id, file_name, bytes)) => {
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|e| e.to_string())?;
                                        latest_clipboard_file_offer_id = Some(transfer_id.clone());
                                        queue_or_mark_file_sending(
                                            &shared,
                                            session.outbound_file_progress().is_some(),
                                            &mut queued_file_status,
                                            QueuedFileStatus {
                                                summary,
                                                transfer_id,
                                                file_name,
                                                bytes,
                                            },
                                        );
                                        clip.adopt_text_baseline();
                                        handled_text_path = true;
                                    }
                                    Err(err) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!("path text skip: {err}"));
                                        });
                                    }
                                }
                            }
                        }
                        if !handled_text_path {
                            content_seq += 1;
                            let cid = format!("ui-clip-{}-{content_seq}", std::process::id());
                            if let Ok(QueueClipboardResult::Queued) =
                                session.queue_clipboard_text(cid, text.clone())
                            {
                                conn.send_all(session.take_outbox().iter())
                                    .map_err(|e| e.to_string())?;
                                with_status(&shared, |s| {
                                    s.last_sync_text = Some(text);
                                });
                            }
                        }
                    }
                }
                match clip.poll_image_change() {
                    Ok(Some(image)) => {
                        latest_clipboard_file_offer_id = None;
                        content_seq += 1;
                        let cid = format!("ui-clip-img-{}-{content_seq}", std::process::id());
                        match image.prepare_inline(m590_core::Session::INLINE_IMAGE_MAX_BYTES) {
                            Ok((encoding, data)) => {
                                let summary = format!(
                                    "[image {}x{} {}B {encoding:?}]",
                                    image.width,
                                    image.height,
                                    data.len()
                                );
                                match session.queue_clipboard_image_encoded(
                                    cid,
                                    image.width,
                                    image.height,
                                    encoding,
                                    data,
                                ) {
                                    Ok(QueueClipboardResult::Queued) => {
                                        conn.send_all(session.take_outbox().iter())
                                            .map_err(|e| e.to_string())?;
                                        with_status(&shared, |s| {
                                            s.last_sync_text = Some(summary);
                                            s.last_error = None;
                                        });
                                    }
                                    Ok(QueueClipboardResult::ImageTooLarge { byte_len, limit }) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!(
                                                "图片过大已跳过 {byte_len}B > {limit}B"
                                            ));
                                        });
                                    }
                                    Ok(_) => {}
                                    Err(err) => {
                                        with_status(&shared, |s| {
                                            s.last_error = Some(format!("queue_image: {err}"));
                                        });
                                    }
                                }
                            }
                            Err(err) => {
                                with_status(&shared, |s| {
                                    s.last_error = Some(format!("prepare_image: {err}"));
                                });
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        with_status(&shared, |s| {
                            s.last_error = Some(format!("clipboard_image_poll: {err}"));
                        });
                    }
                }
            }
        }

        update_batch_progress(
            &shared,
            session,
            outbound_batch.as_ref(),
            inbound_batch.as_ref(),
        );

        match session_loop_pause(session.has_active_file_transfer(), file_progressed) {
            SessionLoopPause::Yield => thread::yield_now(),
            SessionLoopPause::Sleep(delay) => thread::sleep(delay),
        }
    }
}

fn cancel_runtime_batch(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    outbound: &mut Option<OutboundBatchState>,
    inbound: &mut Option<InboundBatchState>,
    reason: &str,
) -> Result<(), String> {
    let mut ids = Vec::new();
    let mut cancelled_batch_id = None;
    if let Some(state) = outbound.take() {
        cancelled_batch_id = Some(state.batch_id.clone());
        ids.extend(state.pending_ids);
    }
    if let Some(state) = inbound.take() {
        cancelled_batch_id = Some(state.batch_id.clone());
        ids.extend(state.pending_ids.iter().cloned());
    }
    if ids.is_empty() && cancelled_batch_id.is_none() {
        return Ok(());
    }
    cancel_transfer_ids(session, conn, ids, reason)?;
    with_status(shared, |status| {
        status.file_transfer_phase = Some("cancelled".into());
        if let Some(batch_id) = cancelled_batch_id {
            status.last_file_transfer_id = Some(batch_id);
        }
        status.file_batch_current_path = None;
        status.file_bytes_received = None;
        status.file_bytes_total = None;
        status.last_error = None;
    });
    Ok(())
}

fn cancel_transfer_ids(
    session: &mut Session,
    conn: &mut TcpFrameStream,
    ids: impl IntoIterator<Item = String>,
    reason: &str,
) -> Result<(), String> {
    for transfer_id in ids {
        session
            .cancel_file(transfer_id, reason)
            .map_err(|err| err.to_string())?;
        let outbox = session.take_outbox();
        conn.send_all(outbox.iter())
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn mark_outbound_batch_started(shared: &SharedStatus, batch: &OutboundBatchState) {
    let current = batch.files.first();
    with_status(shared, |status| {
        status.file_transfer_phase = Some("sending".into());
        status.last_file_transfer_id = Some(batch.batch_id.clone());
        status.last_file_name = Some(batch.display_name.clone());
        status.last_file_bytes = Some(batch.total_bytes);
        status.last_file_saved_path = None;
        status.file_bytes_received = Some(0);
        status.file_bytes_total = current.map(|entry| entry.size);
        status.file_batch_id = Some(batch.batch_id.clone());
        status.file_batch_name = Some(batch.display_name.clone());
        status.file_batch_files_completed = Some(0);
        status.file_batch_files_total = Some(batch.files.len() as u32);
        status.file_batch_bytes_completed = Some(0);
        status.file_batch_bytes_total = Some(batch.total_bytes);
        status.file_batch_current_path = current.map(|entry| entry.relative_path.clone());
        status.last_sync_text = Some(format!(
            "[batch offer {} files {}B id={}]",
            batch.files.len(),
            batch.total_bytes,
            batch.batch_id
        ));
        status.last_error = None;
    });
}

fn mark_inbound_batch_started(shared: &SharedStatus, batch: &InboundBatchState) {
    let current = batch.current();
    with_status(shared, |status| {
        status.file_transfer_phase = Some("receiving".into());
        status.last_file_transfer_id = Some(batch.batch_id.clone());
        status.last_file_name = Some(batch.display_name.clone());
        status.last_file_bytes = Some(batch.total_bytes);
        status.last_file_saved_path = None;
        status.file_bytes_received = Some(0);
        status.file_bytes_total = current.map(|entry| entry.size);
        status.file_batch_id = Some(batch.batch_id.clone());
        status.file_batch_name = Some(batch.display_name.clone());
        status.file_batch_files_completed = Some(0);
        status.file_batch_files_total = Some(batch.files.len() as u32);
        status.file_batch_bytes_completed = Some(0);
        status.file_batch_bytes_total = Some(batch.total_bytes);
        status.file_batch_current_path = current.map(|entry| entry.relative_path.clone());
        status.last_sync_text = Some(format!(
            "[batch receiving {} files {}B id={}]",
            batch.files.len(),
            batch.total_bytes,
            batch.batch_id
        ));
        status.last_error = None;
    });
}

fn mark_batch_done(shared: &SharedStatus, batch_id: &str, saved_path: Option<&Path>) {
    with_status(shared, |status| {
        if status.file_batch_id.as_deref() != Some(batch_id) {
            return;
        }
        status.file_transfer_phase = Some("done".into());
        status.last_file_transfer_id = Some(batch_id.to_string());
        status.file_batch_files_completed = status.file_batch_files_total;
        status.file_batch_bytes_completed = status.file_batch_bytes_total;
        status.file_batch_current_path = None;
        status.file_bytes_received = status.file_bytes_total;
        status.last_file_saved_path = saved_path.map(|path| path.display().to_string());
        status.last_error = None;
    });
}

fn mark_batch_failed(status: &mut HubStatus, batch_id: &str, message: &str) {
    status.file_transfer_phase = Some("failed".into());
    status.last_file_transfer_id = Some(batch_id.to_string());
    status.file_batch_id = Some(batch_id.to_string());
    status.file_batch_current_path = None;
    status.file_bytes_received = None;
    status.file_bytes_total = None;
    status.last_error = Some(format!("batch transfer failed: {message}"));
}

fn note_outbound_batch_request(
    shared: &SharedStatus,
    message: &Message,
    batch: Option<&OutboundBatchState>,
) {
    let (Some(batch), Message::FileRequest(request)) = (batch, message) else {
        return;
    };
    let Some(entry) = batch.entry(&request.transfer_id) else {
        return;
    };
    if !batch.pending_ids.contains(&entry.entry_id) {
        return;
    }
    with_status(shared, |status| {
        status.file_transfer_phase = Some("sending".into());
        status.last_file_transfer_id = Some(batch.batch_id.clone());
        status.file_batch_current_path = Some(entry.relative_path.clone());
        status.file_bytes_received = Some(0);
        status.file_bytes_total = Some(entry.size);
        status.last_error = None;
    });
}

fn note_outbound_batch_completes(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    outbox: &[Message],
    batch: &mut Option<OutboundBatchState>,
) -> Result<(), String> {
    for message in outbox {
        let Message::FileComplete(complete) = message else {
            continue;
        };
        let Some(state) = batch.as_mut() else {
            continue;
        };
        let Some(entry) = state.entry(&complete.transfer_id).cloned() else {
            continue;
        };
        if !state.pending_ids.remove(&complete.transfer_id) {
            continue;
        }
        if !complete.ok {
            let failed = batch.take().expect("outbound batch exists");
            cancel_transfer_ids(
                session,
                conn,
                failed.pending_ids,
                "batch failed while sending",
            )?;
            with_status(shared, |status| {
                mark_batch_failed(status, &failed.batch_id, &complete.message);
            });
            return Ok(());
        }

        state.completed_files = state.completed_files.saturating_add(1);
        state.completed_bytes = state.completed_bytes.saturating_add(entry.size);
        let next = state
            .files
            .iter()
            .find(|candidate| state.pending_ids.contains(&candidate.entry_id));
        with_status(shared, |status| {
            if status.file_batch_id.as_deref() != Some(state.batch_id.as_str()) {
                return;
            }
            status.file_batch_files_completed = Some(state.completed_files);
            status.file_batch_bytes_completed = Some(state.completed_bytes);
            status.file_batch_current_path = next.map(|entry| entry.relative_path.clone());
            status.file_bytes_received = Some(0);
            status.file_bytes_total = next.map(|entry| entry.size);
        });
        if state.pending_ids.is_empty() {
            let finished = batch.take().expect("outbound batch exists");
            mark_batch_done(shared, &finished.batch_id, None);
            return Ok(());
        }
    }
    Ok(())
}

fn handle_batch_file_event(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    event: &InboundFileResult,
    outbound: &mut Option<OutboundBatchState>,
    inbound: &mut Option<InboundBatchState>,
) -> Result<bool, String> {
    match event {
        InboundFileResult::BatchOffered {
            batch_id,
            display_name,
            entries,
        } => {
            cancel_runtime_batch(
                shared,
                session,
                conn,
                outbound,
                inbound,
                "replaced by a newer remote batch",
            )?;
            let save_dir = with_status(shared, |status| PathBuf::from(&status.file_save_dir));
            let mut state = match prepare_inbound_batch(
                batch_id.clone(),
                display_name.clone(),
                entries.clone(),
                save_dir,
            ) {
                Ok(state) => state,
                Err(error) => {
                    let ids = entries
                        .iter()
                        .filter(|entry| entry.kind == BatchEntryKind::File)
                        .map(|entry| entry.entry_id.clone());
                    cancel_transfer_ids(session, conn, ids, "cannot prepare batch destination")?;
                    notify_batch_failure(session, conn, batch_id, &error)?;
                    with_status(shared, |status| {
                        mark_batch_failed(status, batch_id, &error);
                    });
                    return Ok(true);
                }
            };
            mark_inbound_batch_started(shared, &state);
            if state.files.is_empty() {
                match commit_inbound_batch(&mut state) {
                    Ok(saved) => mark_batch_done(shared, batch_id, Some(&saved)),
                    Err(error) => {
                        notify_batch_failure(session, conn, batch_id, &error)?;
                        with_status(shared, |status| {
                            mark_batch_failed(status, batch_id, &error);
                        });
                    }
                }
                return Ok(true);
            }
            if let Err(error) = request_current_batch_file(shared, session, conn, &state) {
                let ids = state.pending_ids.iter().cloned().collect::<Vec<_>>();
                cancel_transfer_ids(session, conn, ids, "cannot request batch file")?;
                with_status(shared, |status| {
                    mark_batch_failed(status, batch_id, &error);
                });
                return Ok(true);
            }
            *inbound = Some(state);
            Ok(true)
        }
        InboundFileResult::Applied {
            transfer_id,
            file_name,
            path,
            size,
            ..
        } if inbound.as_ref().is_some_and(|state| {
            state
                .current()
                .is_some_and(|entry| entry.entry_id == *transfer_id)
        }) =>
        {
            let mut state = inbound.take().expect("inbound batch exists");
            let current = state.current().cloned().expect("current batch file");
            let result = stage_completed_batch_file(&state, &current, file_name, path, *size);
            if let Err(error) = result {
                state.pending_ids.remove(transfer_id);
                let ids = state.pending_ids.iter().cloned().collect::<Vec<_>>();
                cancel_transfer_ids(session, conn, ids, "batch staging failed")?;
                with_status(shared, |status| {
                    mark_batch_failed(status, &state.batch_id, &error);
                });
                return Ok(true);
            }

            state.pending_ids.remove(transfer_id);
            state.completed_files = state.completed_files.saturating_add(1);
            state.completed_bytes = state.completed_bytes.saturating_add(*size);
            state.current_index += 1;
            if state.current().is_some() {
                with_status(shared, |status| {
                    status.file_batch_files_completed = Some(state.completed_files);
                    status.file_batch_bytes_completed = Some(state.completed_bytes);
                });
                if let Err(error) = request_current_batch_file(shared, session, conn, &state) {
                    let ids = state.pending_ids.iter().cloned().collect::<Vec<_>>();
                    cancel_transfer_ids(session, conn, ids, "cannot request batch file")?;
                    with_status(shared, |status| {
                        mark_batch_failed(status, &state.batch_id, &error);
                    });
                    return Ok(true);
                }
                *inbound = Some(state);
            } else {
                let batch_id = state.batch_id.clone();
                match commit_inbound_batch(&mut state) {
                    Ok(saved) => mark_batch_done(shared, &batch_id, Some(&saved)),
                    Err(error) => {
                        notify_batch_failure(session, conn, &batch_id, &error)?;
                        with_status(shared, |status| {
                            mark_batch_failed(status, &batch_id, &error);
                        });
                    }
                }
            }
            Ok(true)
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if inbound
            .as_ref()
            .is_some_and(|state| state.pending_ids.contains(transfer_id)) =>
        {
            let mut failed = inbound.take().expect("inbound batch exists");
            failed.pending_ids.remove(transfer_id);
            let ids = failed.pending_ids.iter().cloned().collect::<Vec<_>>();
            cancel_transfer_ids(session, conn, ids, "batch peer transfer failed")?;
            with_status(shared, |status| {
                mark_batch_failed(status, &failed.batch_id, message);
            });
            Ok(true)
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if outbound
            .as_ref()
            .is_some_and(|state| state.pending_ids.contains(transfer_id)) =>
        {
            let mut failed = outbound.take().expect("outbound batch exists");
            failed.pending_ids.remove(transfer_id);
            cancel_transfer_ids(
                session,
                conn,
                failed.pending_ids,
                "batch peer cancelled transfer",
            )?;
            with_status(shared, |status| {
                mark_batch_failed(status, &failed.batch_id, message);
            });
            Ok(true)
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if outbound
            .as_ref()
            .is_some_and(|state| state.batch_id == *transfer_id)
            || inbound
                .as_ref()
                .is_some_and(|state| state.batch_id == *transfer_id)
            || with_status(shared, |status| {
                status.file_batch_id.as_deref() == Some(transfer_id.as_str())
            }) =>
        {
            cancel_runtime_batch(
                shared,
                session,
                conn,
                outbound,
                inbound,
                "batch failed on peer",
            )?;
            with_status(shared, |status| {
                mark_batch_failed(status, transfer_id, message);
            });
            Ok(true)
        }
        InboundFileResult::Offered { .. } if outbound.is_some() || inbound.is_some() => {
            cancel_runtime_batch(
                shared,
                session,
                conn,
                outbound,
                inbound,
                "replaced by a single-file offer",
            )?;
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn notify_batch_failure(
    session: &mut Session,
    conn: &mut TcpFrameStream,
    batch_id: &str,
    message: &str,
) -> Result<(), String> {
    session
        .cancel_file(batch_id.to_string(), format!("batch failed: {message}"))
        .map_err(|error| error.to_string())?;
    let outbox = session.take_outbox();
    conn.send_all(outbox.iter())
        .map_err(|error| error.to_string())
}

fn prepare_inbound_batch(
    batch_id: String,
    display_name: String,
    entries: Vec<BatchEntry>,
    save_dir: PathBuf,
) -> Result<InboundBatchState, String> {
    let partial_dir = save_dir.join(".partial");
    let staging_dir = partial_dir.join(format!("{batch_id}.batch"));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .map_err(|error| format!("remove stale batch staging directory: {error}"))?;
    }
    fs::create_dir_all(&staging_dir)
        .map_err(|error| format!("create batch staging directory: {error}"))?;

    let files: Vec<BatchEntry> = entries
        .iter()
        .filter(|entry| entry.kind == BatchEntryKind::File)
        .cloned()
        .collect();
    for entry in &entries {
        let relative = batch_relative_path(&entry.relative_path)?;
        let destination = staging_dir.join(relative);
        if entry.kind == BatchEntryKind::Directory {
            fs::create_dir_all(&destination)
                .map_err(|error| format!("create batch directory: {error}"))?;
        } else if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create batch file parent: {error}"))?;
        }
    }

    Ok(InboundBatchState {
        batch_id,
        display_name,
        pending_ids: files.iter().map(|entry| entry.entry_id.clone()).collect(),
        current_index: 0,
        completed_files: 0,
        completed_bytes: 0,
        total_bytes: files.iter().map(|entry| entry.size).sum(),
        files,
        entries,
        save_dir,
        partial_dir,
        staging_dir,
        committed: false,
    })
}

fn request_current_batch_file(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    batch: &InboundBatchState,
) -> Result<(), String> {
    let current = batch
        .current()
        .ok_or_else(|| "batch has no current file".to_string())?;
    match session.request_file(current.entry_id.clone()) {
        Ok(QueueFileResult::Queued) => {
            let outbox = session.take_outbox();
            conn.send_all(outbox.iter())
                .map_err(|error| error.to_string())?;
            with_status(shared, |status| {
                status.file_transfer_phase = Some("receiving".into());
                status.last_file_transfer_id = Some(batch.batch_id.clone());
                status.file_batch_current_path = Some(current.relative_path.clone());
                status.file_bytes_received = Some(0);
                status.file_bytes_total = Some(current.size);
                status.last_error = None;
            });
            Ok(())
        }
        Ok(other) => Err(format!("request batch file failed: {other:?}")),
        Err(error) => Err(format!("request batch file error: {error}")),
    }
}

fn stage_completed_batch_file(
    batch: &InboundBatchState,
    entry: &BatchEntry,
    file_name: &str,
    part_path: &Path,
    size: u64,
) -> Result<(), String> {
    if file_name != entry.relative_path || size != entry.size {
        return Err("completed batch file does not match manifest".into());
    }
    let expected_part = batch.partial_dir.join(format!("{}.part", entry.entry_id));
    if part_path != expected_part {
        return Err("completed batch file escaped partial directory".into());
    }
    let destination = batch
        .staging_dir
        .join(batch_relative_path(&entry.relative_path)?);
    if destination.exists() {
        return Err(format!(
            "duplicate batch staging destination: {}",
            entry.relative_path
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create batch destination parent: {error}"))?;
    }
    fs::rename(part_path, &destination)
        .map_err(|error| format!("stage completed batch file: {error}"))
}

fn commit_inbound_batch(batch: &mut InboundBatchState) -> Result<PathBuf, String> {
    let top_levels: HashSet<String> = batch
        .entries
        .iter()
        .filter_map(|entry| entry.relative_path.split('/').next().map(str::to_owned))
        .collect();
    let (source, destination_name, remove_staging_parent) = if top_levels.len() == 1 {
        let top = top_levels.iter().next().expect("one batch top-level");
        (batch.staging_dir.join(top), top.clone(), true)
    } else {
        (
            batch.staging_dir.clone(),
            safe_batch_container_name(&batch.display_name),
            false,
        )
    };
    let destination = file_save::unique_save_path(&batch.save_dir, &destination_name)?;
    fs::rename(&source, &destination)
        .map_err(|error| format!("publish completed batch: {error}"))?;
    if remove_staging_parent {
        let _ = fs::remove_dir(&batch.staging_dir);
    }
    batch.committed = true;
    Ok(destination)
}

fn batch_relative_path(relative_path: &str) -> Result<PathBuf, String> {
    m590_core::validate_batch_relative_path(relative_path).map_err(|error| error.to_string())?;
    let mut path = PathBuf::new();
    for component in relative_path.split('/') {
        path.push(component);
    }
    Ok(path)
}

fn safe_batch_container_name(display_name: &str) -> String {
    let sanitized: String = display_name
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() || matches!(sanitized, "." | "..") {
        "M590Bridge-batch".into()
    } else {
        sanitized.to_string()
    }
}

fn update_batch_progress(
    shared: &SharedStatus,
    session: &Session,
    outbound: Option<&OutboundBatchState>,
    inbound: Option<&InboundBatchState>,
) {
    if let Some(batch) = outbound {
        if let Some((transfer_id, sent, total)) = session.outbound_file_progress() {
            if let Some(entry) = batch.entry(&transfer_id) {
                with_status(shared, |status| {
                    status.file_transfer_phase = Some("sending".into());
                    status.last_file_transfer_id = Some(batch.batch_id.clone());
                    status.file_batch_current_path = Some(entry.relative_path.clone());
                    status.file_bytes_received = Some(sent);
                    status.file_bytes_total = Some(total);
                });
            }
        }
        return;
    }
    if let Some(batch) = inbound {
        let Some(current) = batch.current() else {
            return;
        };
        if let Some((transfer_id, received, total)) = session.inbound_file_progress() {
            if transfer_id == current.entry_id {
                with_status(shared, |status| {
                    status.file_transfer_phase = Some("receiving".into());
                    status.last_file_transfer_id = Some(batch.batch_id.clone());
                    status.file_batch_current_path = Some(current.relative_path.clone());
                    status.file_bytes_received = Some(received);
                    status.file_bytes_total = Some(total);
                });
            }
        }
    }
}

fn clear_batch_status_fields(status: &mut HubStatus) {
    status.file_batch_id = None;
    status.file_batch_name = None;
    status.file_batch_files_completed = None;
    status.file_batch_files_total = None;
    status.file_batch_bytes_completed = None;
    status.file_batch_bytes_total = None;
    status.file_batch_current_path = None;
}

fn mark_file_sending(
    shared: &SharedStatus,
    summary: String,
    transfer_id: String,
    file_name: String,
    bytes: u64,
) {
    with_status(shared, |s| {
        clear_batch_status_fields(s);
        s.file_transfer_phase = Some("sending".into());
        s.last_file_transfer_id = Some(transfer_id);
        s.last_file_name = Some(file_name);
        s.last_file_bytes = Some(bytes);
        s.last_file_saved_path = None;
        s.file_bytes_received = Some(0);
        s.file_bytes_total = Some(bytes);
        s.last_sync_text = Some(summary);
        s.last_error = None;
    });
}

fn mark_queued_file_sending(shared: &SharedStatus, status: QueuedFileStatus) {
    mark_file_sending(
        shared,
        status.summary,
        status.transfer_id,
        status.file_name,
        status.bytes,
    );
}

fn queue_or_mark_file_sending(
    shared: &SharedStatus,
    outbound_active: bool,
    queued: &mut Option<QueuedFileStatus>,
    status: QueuedFileStatus,
) {
    if outbound_active {
        *queued = Some(status);
    } else {
        mark_queued_file_sending(shared, status);
    }
}

fn promote_queued_file_status_after(
    shared: &SharedStatus,
    queued: &mut Option<QueuedFileStatus>,
    finished_transfer_id: &str,
) -> bool {
    let current_finished = with_status(shared, |status| {
        status.last_file_transfer_id.as_deref() == Some(finished_transfer_id)
    });
    if !current_finished {
        return false;
    }
    if let Some(next) = queued.take() {
        mark_queued_file_sending(shared, next);
        true
    } else {
        false
    }
}

fn discard_queued_file_status(
    queued: &mut Option<QueuedFileStatus>,
    failed_transfer_id: &str,
) -> bool {
    if queued
        .as_ref()
        .is_some_and(|status| status.transfer_id == failed_transfer_id)
    {
        queued.take();
        true
    } else {
        false
    }
}

/// Sender side: after FileRequest is answered, outbox contains FileComplete but status
/// used to stay on `sending` / 0%. Mirror complete into hub status for UI progress.
fn note_outbound_file_completes(
    shared: &SharedStatus,
    outbox: &[Message],
    queued: &mut Option<QueuedFileStatus>,
) -> Vec<(String, bool)> {
    let mut resolutions = Vec::new();
    for msg in outbox {
        let Message::FileComplete(payload) = msg else {
            continue;
        };
        resolutions.push((payload.transfer_id.clone(), payload.ok));
        let updated = with_status(shared, |s| {
            let matches_current = s
                .last_file_transfer_id
                .as_deref()
                .is_some_and(|id| id == payload.transfer_id);
            if !matches_current {
                return false;
            }
            s.last_file_transfer_id = Some(payload.transfer_id.clone());
            if payload.ok {
                s.file_transfer_phase = Some("done".into());
                if let Some(total) = s.file_bytes_total.or(s.last_file_bytes) {
                    s.file_bytes_received = Some(total);
                    s.file_bytes_total = Some(total);
                }
                s.last_error = None;
            } else {
                s.file_transfer_phase = Some("failed".into());
                s.last_error = Some(payload.message.clone());
            }
            true
        });
        if updated {
            promote_queued_file_status_after(shared, queued, &payload.transfer_id);
        }
    }
    resolutions
}

fn rearm_completed_clipboard_offers(
    clipboard: Option<&mut PlatformClipboard>,
    latest_clipboard_offer_id: &mut Option<String>,
    resolutions: &[(String, bool)],
) {
    let should_rearm = resolutions.iter().any(|(transfer_id, ok)| {
        *ok && latest_clipboard_offer_id.as_deref() == Some(transfer_id.as_str())
    });
    if should_rearm {
        *latest_clipboard_offer_id = None;
        if let Some(clipboard) = clipboard {
            clipboard.rearm_file_offer_poll();
        }
    }
}

fn file_failure_rearms_clipboard_offer(message: &str) -> bool {
    matches!(
        message,
        "virtual file reader closed"
            | "virtual file read timeout"
            | "virtual file consumer stalled"
    )
}

fn rearm_cancelled_clipboard_offer(
    clipboard: Option<&mut PlatformClipboard>,
    latest_clipboard_offer_id: &mut Option<String>,
    transfer_id: &str,
) {
    if latest_clipboard_offer_id.as_deref() != Some(transfer_id) {
        return;
    }
    *latest_clipboard_offer_id = None;
    if let Some(clipboard) = clipboard {
        clipboard.rearm_file_offer_poll();
    }
}

#[cfg(target_os = "windows")]
fn publish_windows_virtual_offer(
    manager: &WindowsVirtualFileManager,
    offer: &DeferredVirtualOffer,
) -> Result<WindowsVirtualReceive, String> {
    let (file, receive) = prepare_windows_virtual_offer(offer)?;
    manager.publish(file)?;
    Ok(receive)
}

#[cfg(target_os = "windows")]
fn replace_windows_virtual_offer_if_current(
    manager: &WindowsVirtualFileManager,
    offer: &DeferredVirtualOffer,
) -> Result<Option<WindowsVirtualReceive>, String> {
    let (file, receive) = prepare_windows_virtual_offer(offer)?;
    Ok(manager.replace_if_current(file)?.then_some(receive))
}

#[cfg(target_os = "windows")]
fn prepare_windows_virtual_offer(
    offer: &DeferredVirtualOffer,
) -> Result<(m590_clipboard::VirtualFile, WindowsVirtualReceive), String> {
    let (bridge, producer) = VirtualFileBridge::new();
    let file = bridge
        .virtual_file(offer.file_name.clone(), offer.size)
        .map_err(|e| e.to_string())?;
    Ok((
        file,
        WindowsVirtualReceive {
            transfer_id: offer.transfer_id.clone(),
            file_name: offer.file_name.clone(),
            size: offer.size,
            bridge,
            producer,
            requested: false,
            completed: false,
            clipboard_replaced: false,
            published_at: Instant::now(),
            requested_at: None,
            network_started_at: None,
            first_chunk_at: None,
        },
    ))
}

#[cfg(target_os = "windows")]
fn publish_windows_virtual_batch_offer(
    manager: &WindowsVirtualFileManager,
    offer: &DeferredVirtualBatchOffer,
) -> Result<WindowsVirtualBatchReceive, String> {
    let (collection, receive) = prepare_windows_virtual_batch_offer(offer)?;
    manager.publish_collection(collection)?;
    Ok(receive)
}

#[cfg(target_os = "windows")]
fn replace_windows_virtual_batch_offer_if_current(
    manager: &WindowsVirtualFileManager,
    offer: &DeferredVirtualBatchOffer,
) -> Result<Option<WindowsVirtualBatchReceive>, String> {
    let (collection, receive) = prepare_windows_virtual_batch_offer(offer)?;
    Ok(manager
        .replace_collection_if_current(collection)?
        .then_some(receive))
}

#[cfg(target_os = "windows")]
fn prepare_windows_virtual_batch_offer(
    offer: &DeferredVirtualBatchOffer,
) -> Result<
    (
        m590_clipboard::VirtualFileCollection,
        WindowsVirtualBatchReceive,
    ),
    String,
> {
    let mut collection_entries = Vec::with_capacity(offer.entries.len());
    let mut files = Vec::new();
    for (descriptor_index, entry) in offer.entries.iter().enumerate() {
        match entry.kind {
            BatchEntryKind::Directory => {
                let descriptor =
                    m590_clipboard::VirtualFileCollectionEntry::directory(&entry.relative_path)
                        .map_err(|error| error.to_string())?;
                collection_entries.push(descriptor);
            }
            BatchEntryKind::File => {
                let file_name = entry
                    .relative_path
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| "batch file name missing".to_string())?;
                let (bridge, producer) = VirtualFileBridge::new();
                let file = bridge
                    .virtual_file(file_name.to_string(), entry.size)
                    .map_err(|error| error.to_string())?;
                collection_entries.push(
                    m590_clipboard::VirtualFileCollectionEntry::file(&entry.relative_path, file)
                        .map_err(|error| error.to_string())?,
                );
                files.push(WindowsVirtualBatchFile {
                    descriptor_index,
                    entry: entry.clone(),
                    bridge,
                    producer,
                    requested: false,
                    completed: false,
                    requested_at: None,
                    network_started_at: None,
                    first_chunk_at: None,
                });
            }
        }
    }
    let collection = m590_clipboard::VirtualFileCollection::new(collection_entries)
        .map_err(|error| error.to_string())?;
    Ok((
        collection,
        WindowsVirtualBatchReceive {
            batch_id: offer.batch_id.clone(),
            files,
            published_at: Instant::now(),
            active_index: None,
            completed_files: 0,
            completed_bytes: 0,
            clipboard_replaced: false,
            clipboard_replaced_idle_since: None,
        },
    ))
}

#[cfg(target_os = "windows")]
fn cancel_windows_virtual_batch(
    session: &mut Session,
    conn: &mut TcpFrameStream,
    batch: &WindowsVirtualBatchReceive,
    reason: &str,
) -> Result<(), String> {
    for file in &batch.files {
        if !file.completed {
            file.producer.fail(reason);
        }
    }
    cancel_transfer_ids(session, conn, batch.pending_ids(), reason)
}

#[cfg(target_os = "windows")]
fn cancel_deferred_windows_virtual_batch(
    session: &mut Session,
    conn: &mut TcpFrameStream,
    offer: DeferredVirtualBatchOffer,
    reason: &str,
) -> Result<(), String> {
    let file_ids = offer.file_ids().collect::<Vec<_>>();
    if file_ids.is_empty() {
        session
            .cancel_file(offer.batch_id, reason)
            .map_err(|error| error.to_string())?;
        return conn
            .send_all(session.take_outbox().iter())
            .map_err(|error| error.to_string());
    }
    cancel_transfer_ids(session, conn, file_ids, reason)
}

#[cfg(target_os = "windows")]
fn mark_windows_virtual_batch_offer(shared: &SharedStatus, offer: &DeferredVirtualBatchOffer) {
    let files = offer
        .entries
        .iter()
        .filter(|entry| entry.kind == BatchEntryKind::File)
        .count() as u32;
    let total_bytes = offer
        .entries
        .iter()
        .filter(|entry| entry.kind == BatchEntryKind::File)
        .map(|entry| entry.size)
        .sum::<u64>();
    with_status(shared, |status| {
        status.file_transfer_phase = Some("offered".into());
        status.last_file_transfer_id = Some(offer.batch_id.clone());
        status.last_file_name = Some(offer.display_name.clone());
        status.last_file_bytes = Some(total_bytes);
        status.last_file_saved_path = None;
        status.file_bytes_received = Some(0);
        status.file_bytes_total = Some(total_bytes);
        status.file_batch_id = Some(offer.batch_id.clone());
        status.file_batch_name = Some(offer.display_name.clone());
        status.file_batch_files_completed = Some(0);
        status.file_batch_files_total = Some(files);
        status.file_batch_bytes_completed = Some(0);
        status.file_batch_bytes_total = Some(total_bytes);
        status.file_batch_current_path = offer
            .entries
            .iter()
            .find(|entry| entry.kind == BatchEntryKind::File)
            .map(|entry| entry.relative_path.clone());
        status.last_sync_text = Some(format!(
            "[batch file offer {} files {}B id={}]",
            files, total_bytes, offer.batch_id
        ));
        status.last_error = None;
    });
}

#[cfg(target_os = "windows")]
fn handle_windows_virtual_batch_stream_event(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    manager: &WindowsVirtualFileManager,
    event: &InboundFileResult,
    receive: &mut Option<WindowsVirtualBatchReceive>,
    deferred: &mut Option<DeferredVirtualBatchOffer>,
) -> Result<bool, String> {
    match event {
        InboundFileResult::Chunk { transfer_id, data }
            if receive
                .as_ref()
                .is_some_and(|batch| batch.file_index(transfer_id).is_some()) =>
        {
            let batch = receive.as_mut().expect("matching Windows virtual batch");
            let index = batch
                .file_index(transfer_id)
                .expect("matching Windows virtual batch file");
            if batch.files[index].first_chunk_at.is_none() {
                let first_chunk_at = Instant::now();
                batch.files[index].first_chunk_at = Some(first_chunk_at);
                let first_byte_ms = batch.files[index]
                    .network_started_at
                    .map(|started| {
                        first_chunk_at
                            .saturating_duration_since(started)
                            .as_millis()
                    })
                    .unwrap_or(0);
                task_057_diagnostic(format_args!(
                    "network_first_chunk batch_id={:?} lindex={} entry_id={transfer_id:?} path={:?} bytes={} first_byte_ms={first_byte_ms}",
                    batch.batch_id,
                    batch.files[index].descriptor_index,
                    batch.files[index].entry.relative_path,
                    data.len()
                ));
            }
            let push_error = batch.files[index]
                .producer
                .push(data)
                .err()
                .map(|error| format!("virtual batch stream: {error}"));
            let (received, total) = session
                .inbound_file_progress()
                .filter(|(id, _, _)| id == transfer_id)
                .map(|(_, received, total)| (received, total))
                .unwrap_or_else(|| {
                    let received =
                        with_status(shared, |status| status.file_bytes_received.unwrap_or(0));
                    (
                        received.saturating_add(data.len() as u64),
                        batch.files[index].entry.size,
                    )
                });
            with_status(shared, |status| {
                status.file_transfer_phase = Some("receiving".into());
                status.last_file_transfer_id = Some(batch.batch_id.clone());
                status.file_batch_current_path =
                    Some(batch.files[index].entry.relative_path.clone());
                status.file_bytes_received = Some(received);
                status.file_bytes_total = Some(total);
                status.last_error = push_error;
            });
            Ok(true)
        }
        InboundFileResult::StreamCompleted {
            transfer_id, size, ..
        } if receive
            .as_ref()
            .is_some_and(|batch| batch.file_index(transfer_id).is_some()) =>
        {
            let batch = receive.as_mut().expect("matching Windows virtual batch");
            let index = batch
                .file_index(transfer_id)
                .expect("matching Windows virtual batch file");
            let completed_at = Instant::now();
            let network_elapsed = batch.files[index]
                .network_started_at
                .map(|started| completed_at.saturating_duration_since(started));
            let data_elapsed = batch.files[index]
                .first_chunk_at
                .map(|started| completed_at.saturating_duration_since(started));
            let effective_mib_per_second = network_elapsed
                .filter(|elapsed| !elapsed.is_zero())
                .map(|elapsed| *size as f64 / 1_048_576.0 / elapsed.as_secs_f64())
                .unwrap_or(0.0);
            let data_mib_per_second = data_elapsed
                .filter(|elapsed| !elapsed.is_zero())
                .map(|elapsed| *size as f64 / 1_048_576.0 / elapsed.as_secs_f64())
                .unwrap_or(0.0);
            task_057_diagnostic(format_args!(
                "network_stream_completed batch_id={:?} lindex={} entry_id={transfer_id:?} path={:?} size={size} network_ms={} data_ms={} effective_mib_s={effective_mib_per_second:.2} data_mib_s={data_mib_per_second:.2} batch_elapsed_ms={}",
                batch.batch_id,
                batch.files[index].descriptor_index,
                batch.files[index].entry.relative_path,
                network_elapsed.map_or(0, |elapsed| elapsed.as_millis()),
                data_elapsed.map_or(0, |elapsed| elapsed.as_millis()),
                completed_at
                    .saturating_duration_since(batch.published_at)
                    .as_millis()
            ));
            if !batch.files[index].completed {
                batch.files[index].producer.finish();
                batch.files[index].completed = true;
                batch.completed_files = batch.completed_files.saturating_add(1);
                batch.completed_bytes = batch.completed_bytes.saturating_add(*size);
            }
            if batch.active_index == Some(index) {
                batch.active_index = None;
            }
            let next = batch
                .files
                .iter()
                .find(|file| !file.completed)
                .map(|file| file.entry.relative_path.clone());
            with_status(shared, |status| {
                status.file_batch_files_completed = Some(batch.completed_files);
                status.file_batch_bytes_completed = Some(batch.completed_bytes);
                status.file_batch_current_path = next;
                status.file_bytes_received = Some(*size);
                status.file_bytes_total = Some(*size);
                status.last_error = None;
            });
            if batch.is_complete() {
                mark_batch_done(shared, &batch.batch_id, None);
            }
            Ok(true)
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if receive.as_ref().is_some_and(|batch| {
            batch.batch_id == *transfer_id || batch.file_index(transfer_id).is_some()
        }) =>
        {
            let failed = receive.take().expect("matching Windows virtual batch");
            let batch_id = failed.batch_id.clone();
            task_057_diagnostic(format_args!(
                "network_stream_failed batch_id={batch_id:?} transfer_id={transfer_id:?} message={message:?} batch_elapsed_ms={}",
                Instant::now()
                    .saturating_duration_since(failed.published_at)
                    .as_millis()
            ));
            cancel_windows_virtual_batch(session, conn, &failed, message)?;
            manager.clear();
            with_status(shared, |status| {
                mark_batch_failed(status, &batch_id, message);
            });
            Ok(true)
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if deferred.as_ref().is_some_and(|batch| {
            batch.batch_id == *transfer_id
                || batch.entries.iter().any(|entry| {
                    entry.kind == BatchEntryKind::File && entry.entry_id == *transfer_id
                })
        }) =>
        {
            let failed = deferred.take().expect("matching deferred Windows batch");
            let batch_id = failed.batch_id.clone();
            cancel_deferred_windows_virtual_batch(session, conn, failed, message)?;
            with_status(shared, |status| {
                mark_batch_failed(status, &batch_id, message);
            });
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(target_os = "linux")]
fn publish_linux_virtual_batch_offer(
    manager: &mut LinuxVirtualFileManager,
    clipboard: &mut PlatformClipboard,
    offer: &DeferredVirtualBatchOffer,
) -> Result<LinuxVirtualBatchReceive, String> {
    let (tree, receive) = prepare_linux_virtual_batch_offer(offer)?;
    manager
        .publish_tree(clipboard, tree)
        .map_err(|error| error.to_string())?;
    Ok(receive)
}

#[cfg(target_os = "linux")]
fn replace_linux_virtual_batch_offer_if_current(
    manager: &mut LinuxVirtualFileManager,
    clipboard: &mut PlatformClipboard,
    offer: &DeferredVirtualBatchOffer,
) -> Result<Option<LinuxVirtualBatchReceive>, String> {
    let (tree, receive) = prepare_linux_virtual_batch_offer(offer)?;
    Ok(manager
        .replace_tree_if_current(clipboard, tree)
        .map_err(|error| error.to_string())?
        .then_some(receive))
}

#[cfg(target_os = "linux")]
fn prepare_linux_virtual_batch_offer(
    offer: &DeferredVirtualBatchOffer,
) -> Result<
    (
        crate::linux_virtual_file::LinuxVirtualFileTree,
        LinuxVirtualBatchReceive,
    ),
    String,
> {
    use crate::linux_virtual_file::{LinuxVirtualFileTree, LinuxVirtualFileTreeEntry};

    let mut tree_entries = Vec::with_capacity(offer.entries.len());
    let mut files = Vec::new();
    for entry in &offer.entries {
        match entry.kind {
            BatchEntryKind::Directory => {
                tree_entries.push(
                    LinuxVirtualFileTreeEntry::directory(&entry.relative_path)
                        .map_err(|error| error.to_string())?,
                );
            }
            BatchEntryKind::File => {
                let file_name = entry
                    .relative_path
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| "batch file name missing".to_string())?;
                let (bridge, producer) = VirtualFileBridge::new();
                let file = bridge
                    .linux_virtual_file(file_name.to_string(), entry.size)
                    .map_err(|error| error.to_string())?;
                tree_entries.push(
                    LinuxVirtualFileTreeEntry::file(&entry.relative_path, file)
                        .map_err(|error| error.to_string())?,
                );
                files.push(LinuxVirtualBatchFile {
                    entry: entry.clone(),
                    bridge,
                    producer,
                    requested: false,
                    completed: false,
                    consumed: false,
                    released: false,
                });
            }
        }
    }
    let tree = LinuxVirtualFileTree::new(tree_entries).map_err(|error| error.to_string())?;
    Ok((
        tree,
        LinuxVirtualBatchReceive {
            batch_id: offer.batch_id.clone(),
            files,
            active_index: None,
            completed_files: 0,
            completed_bytes: 0,
            clipboard_replaced: false,
        },
    ))
}

#[cfg(target_os = "linux")]
fn cancel_linux_virtual_batch(
    session: &mut Session,
    conn: &mut TcpFrameStream,
    batch: &LinuxVirtualBatchReceive,
    reason: &str,
) -> Result<(), String> {
    for file in &batch.files {
        if !file.completed {
            file.producer.fail(reason);
        }
    }
    cancel_transfer_ids(session, conn, batch.pending_ids(), reason)
}

#[cfg(target_os = "linux")]
fn cancel_deferred_linux_virtual_batch(
    session: &mut Session,
    conn: &mut TcpFrameStream,
    offer: DeferredVirtualBatchOffer,
    reason: &str,
) -> Result<(), String> {
    let file_ids = offer.file_ids().collect::<Vec<_>>();
    if file_ids.is_empty() {
        session
            .cancel_file(offer.batch_id, reason)
            .map_err(|error| error.to_string())?;
        return conn
            .send_all(session.take_outbox().iter())
            .map_err(|error| error.to_string());
    }
    cancel_transfer_ids(session, conn, file_ids, reason)
}

#[cfg(target_os = "linux")]
fn mark_linux_virtual_batch_offer(shared: &SharedStatus, offer: &DeferredVirtualBatchOffer) {
    let files = offer
        .entries
        .iter()
        .filter(|entry| entry.kind == BatchEntryKind::File)
        .count() as u32;
    let total_bytes = offer
        .entries
        .iter()
        .filter(|entry| entry.kind == BatchEntryKind::File)
        .map(|entry| entry.size)
        .sum::<u64>();
    with_status(shared, |status| {
        status.file_transfer_phase = Some("offered".into());
        status.last_file_transfer_id = Some(offer.batch_id.clone());
        status.last_file_name = Some(offer.display_name.clone());
        status.last_file_bytes = Some(total_bytes);
        status.last_file_saved_path = None;
        status.file_bytes_received = Some(0);
        status.file_bytes_total = Some(total_bytes);
        status.file_batch_id = Some(offer.batch_id.clone());
        status.file_batch_name = Some(offer.display_name.clone());
        status.file_batch_files_completed = Some(0);
        status.file_batch_files_total = Some(files);
        status.file_batch_bytes_completed = Some(0);
        status.file_batch_bytes_total = Some(total_bytes);
        status.file_batch_current_path = offer
            .entries
            .iter()
            .find(|entry| entry.kind == BatchEntryKind::File)
            .map(|entry| entry.relative_path.clone());
        status.last_sync_text = Some(format!(
            "[batch file offer {} files {}B id={}]",
            files, total_bytes, offer.batch_id
        ));
        status.last_error = None;
    });
}

#[cfg(target_os = "linux")]
fn handle_linux_virtual_batch_stream_event(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    manager: &mut LinuxVirtualFileManager,
    event: &InboundFileResult,
    receive: &mut Option<LinuxVirtualBatchReceive>,
    deferred: &mut Option<DeferredVirtualBatchOffer>,
) -> Result<LinuxVirtualBatchStreamEvent, String> {
    match event {
        InboundFileResult::Chunk { transfer_id, data }
            if receive
                .as_ref()
                .is_some_and(|batch| batch.file_index(transfer_id).is_some()) =>
        {
            let batch = receive.as_mut().expect("matching Linux virtual batch");
            let index = batch
                .file_index(transfer_id)
                .expect("matching Linux virtual batch file");
            let (push_error, pending_chunk) = match batch.files[index].producer.try_push(data) {
                Ok(true) => (None, None),
                Ok(false) => {
                    let pending = PendingLinuxVirtualChunk {
                        transfer_id: transfer_id.clone(),
                        data: data.clone(),
                    };
                    (None, Some(pending))
                }
                Err(error) => {
                    let pending = PendingLinuxVirtualChunk {
                        transfer_id: transfer_id.clone(),
                        data: data.clone(),
                    };
                    (
                        Some(format!("virtual batch stream: {error}")),
                        Some(pending),
                    )
                }
            };
            let (received, total) = session
                .inbound_file_progress()
                .filter(|(id, _, _)| id == transfer_id)
                .map(|(_, received, total)| (received, total))
                .unwrap_or_else(|| {
                    let received =
                        with_status(shared, |status| status.file_bytes_received.unwrap_or(0));
                    (
                        received.saturating_add(data.len() as u64),
                        batch.files[index].entry.size,
                    )
                });
            with_status(shared, |status| {
                status.file_transfer_phase = Some("receiving".into());
                status.last_file_transfer_id = Some(batch.batch_id.clone());
                status.file_batch_current_path =
                    Some(batch.files[index].entry.relative_path.clone());
                status.file_bytes_received = Some(received);
                status.file_bytes_total = Some(total);
                status.last_error = push_error;
            });
            Ok(LinuxVirtualBatchStreamEvent::Handled(pending_chunk))
        }
        InboundFileResult::StreamCompleted {
            transfer_id, size, ..
        } if receive
            .as_ref()
            .is_some_and(|batch| batch.file_index(transfer_id).is_some()) =>
        {
            let batch = receive.as_mut().expect("matching Linux virtual batch");
            let index = batch
                .file_index(transfer_id)
                .expect("matching Linux virtual batch file");
            if !batch.files[index].completed {
                batch.files[index].producer.finish();
                batch.files[index].completed = true;
                batch.completed_files = batch.completed_files.saturating_add(1);
                batch.completed_bytes = batch.completed_bytes.saturating_add(*size);
            }
            if batch.active_index == Some(index) {
                batch.active_index = None;
            }
            let next = batch
                .files
                .iter()
                .find(|file| !file.completed)
                .map(|file| file.entry.relative_path.clone());
            with_status(shared, |status| {
                status.file_batch_files_completed = Some(batch.completed_files);
                status.file_batch_bytes_completed = Some(batch.completed_bytes);
                status.file_batch_current_path = next;
                status.file_bytes_received = Some(*size);
                status.file_bytes_total = Some(*size);
                status.last_error = None;
            });
            if batch.is_complete() {
                mark_batch_done(shared, &batch.batch_id, None);
            }
            Ok(LinuxVirtualBatchStreamEvent::Handled(None))
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if receive.as_ref().is_some_and(|batch| {
            batch.batch_id == *transfer_id || batch.file_index(transfer_id).is_some()
        }) =>
        {
            let failed = receive.take().expect("matching Linux virtual batch");
            let batch_id = failed.batch_id.clone();
            cancel_linux_virtual_batch(session, conn, &failed, message)?;
            manager.clear();
            with_status(shared, |status| {
                mark_batch_failed(status, &batch_id, message);
            });
            Ok(LinuxVirtualBatchStreamEvent::Handled(None))
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } if deferred.as_ref().is_some_and(|batch| {
            batch.batch_id == *transfer_id
                || batch.entries.iter().any(|entry| {
                    entry.kind == BatchEntryKind::File && entry.entry_id == *transfer_id
                })
        }) =>
        {
            let failed = deferred.take().expect("matching deferred Linux batch");
            let batch_id = failed.batch_id.clone();
            cancel_deferred_linux_virtual_batch(session, conn, failed, message)?;
            with_status(shared, |status| {
                mark_batch_failed(status, &batch_id, message);
            });
            Ok(LinuxVirtualBatchStreamEvent::Handled(None))
        }
        _ => Ok(LinuxVirtualBatchStreamEvent::Unhandled),
    }
}

#[cfg(target_os = "linux")]
fn try_push_linux_virtual_chunk(
    transfer_id: &str,
    data: &[u8],
    single: Option<&LinuxVirtualReceive>,
    batch: Option<&LinuxVirtualBatchReceive>,
) -> Result<LinuxVirtualChunkPush, String> {
    let producer = single
        .filter(|receive| receive.transfer_id == transfer_id)
        .map(|receive| &receive.producer)
        .or_else(|| {
            batch.and_then(|receive| {
                receive
                    .file_index(transfer_id)
                    .map(|index| &receive.files[index].producer)
            })
        });
    let Some(producer) = producer else {
        return Ok(LinuxVirtualChunkPush::NoReceiver);
    };
    producer
        .try_push(data)
        .map(|accepted| {
            if accepted {
                LinuxVirtualChunkPush::Accepted
            } else {
                LinuxVirtualChunkPush::Backpressured
            }
        })
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn publish_linux_virtual_offer(
    manager: &mut LinuxVirtualFileManager,
    clipboard: &mut PlatformClipboard,
    offer: &DeferredVirtualOffer,
) -> Result<LinuxVirtualReceive, String> {
    let (file, receive) = prepare_linux_virtual_offer(offer)?;
    manager
        .publish(clipboard, file)
        .map_err(|error| error.to_string())?;
    Ok(receive)
}

#[cfg(target_os = "linux")]
fn replace_linux_virtual_offer_if_current(
    manager: &mut LinuxVirtualFileManager,
    clipboard: &mut PlatformClipboard,
    offer: &DeferredVirtualOffer,
) -> Result<Option<LinuxVirtualReceive>, String> {
    let (file, receive) = prepare_linux_virtual_offer(offer)?;
    Ok(manager
        .replace_if_current(clipboard, file)
        .map_err(|error| error.to_string())?
        .then_some(receive))
}

#[cfg(target_os = "linux")]
fn prepare_linux_virtual_offer(
    offer: &DeferredVirtualOffer,
) -> Result<
    (
        crate::linux_virtual_file::LinuxVirtualFile,
        LinuxVirtualReceive,
    ),
    String,
> {
    let (bridge, producer) = VirtualFileBridge::new();
    let file = bridge
        .linux_virtual_file(offer.file_name.clone(), offer.size)
        .map_err(|error| error.to_string())?;
    Ok((
        file,
        LinuxVirtualReceive {
            transfer_id: offer.transfer_id.clone(),
            bridge,
            producer,
            requested: false,
            completed: false,
            consumed: false,
            released: false,
            clipboard_replaced: false,
        },
    ))
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn mark_virtual_offer(shared: &SharedStatus, offer: &DeferredVirtualOffer) {
    with_status(shared, |s| {
        clear_batch_status_fields(s);
        s.file_transfer_phase = Some("offered".into());
        s.last_file_transfer_id = Some(offer.transfer_id.clone());
        s.last_file_name = Some(offer.file_name.clone());
        s.last_file_bytes = Some(offer.size);
        s.file_bytes_received = Some(0);
        s.file_bytes_total = Some(offer.size);
        s.last_sync_text = Some(format!(
            "[file offer {} {}B; paste to transfer]",
            offer.file_name, offer.size
        ));
        s.last_error = None;
    });
}

fn mark_file_failed_if_current(status: &mut HubStatus, transfer_id: &str, message: &str) -> bool {
    if status
        .last_file_transfer_id
        .as_deref()
        .is_some_and(|current| current != transfer_id)
    {
        return false;
    }
    status.file_transfer_phase = Some("failed".into());
    status.last_file_transfer_id = Some(transfer_id.to_string());
    status.last_error = Some(format!("file transfer failed: {message}"));
    true
}

fn offer_local_file(
    session: &mut Session,
    content_seq: &mut u64,
    path: &str,
) -> Result<(String, String, String, u64), String> {
    let p = PathBuf::from(path);
    let meta = fs::metadata(&p).map_err(|e| format!("stat file: {e}"))?;
    if !meta.is_file() {
        return Err(format!("not a file: {path}"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "file too large: {}B > limit {MAX_FILE_BYTES}B",
            meta.len()
        ));
    }
    let file_name = p
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "file name missing".to_string())?
        .to_string();
    *content_seq += 1;
    let transfer_id = format!("ui-file-{}-{content_seq}", std::process::id());
    let bytes = meta.len();
    match session.offer_file_path(transfer_id.clone(), &p) {
        Ok(QueueFileResult::Queued) => Ok((
            format!("[file offer {file_name} {bytes}B id={transfer_id}]"),
            transfer_id,
            file_name,
            bytes,
        )),
        Ok(QueueFileResult::DuplicateTransferId) => Err("duplicate transfer id".into()),
        Ok(QueueFileResult::FileTooLarge { byte_len, limit }) => {
            Err(format!("file too large: {byte_len}B > {limit}B"))
        }
        Ok(QueueFileResult::UnknownTransferId) => Err("unexpected unknown transfer".into()),
        Err(err) => Err(err.to_string()),
    }
}

fn offer_file_bytes(
    session: &mut Session,
    content_seq: &mut u64,
    file_name: String,
    data: Vec<u8>,
) -> Result<(String, String, String, u64), String> {
    if data.len() > MAX_MEMORY_FILE_BYTES {
        return Err(format!(
            "file too large for memory offer: {}B > limit {MAX_MEMORY_FILE_BYTES}B (use path send)",
            data.len()
        ));
    }
    *content_seq += 1;
    let transfer_id = format!("ui-file-{}-{content_seq}", std::process::id());
    let bytes = data.len() as u64;
    match session.offer_file(transfer_id.clone(), file_name.clone(), data) {
        Ok(QueueFileResult::Queued) => Ok((
            format!("[file offer {file_name} {bytes}B id={transfer_id}]"),
            transfer_id,
            file_name,
            bytes,
        )),
        Ok(QueueFileResult::DuplicateTransferId) => Err("duplicate transfer id".into()),
        Ok(QueueFileResult::FileTooLarge { byte_len, limit }) => {
            Err(format!("file too large: {byte_len}B > {limit}B"))
        }
        Ok(QueueFileResult::UnknownTransferId) => Err("unexpected unknown transfer".into()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn handle_inbound_file(
    shared: &SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
    event: InboundFileResult,
) -> Result<(), String> {
    match event {
        InboundFileResult::Offered {
            transfer_id,
            file_name,
            size,
        } => {
            with_status(shared, |s| {
                clear_batch_status_fields(s);
                s.file_transfer_phase = Some("offered".into());
                s.last_file_transfer_id = Some(transfer_id.clone());
                s.last_file_name = Some(file_name.clone());
                s.last_file_bytes = Some(size);
                s.last_file_saved_path = None;
                s.file_bytes_received = Some(0);
                s.file_bytes_total = Some(size);
                s.last_sync_text = Some(format!("[file offer {file_name} {size}B]"));
                s.last_error = None;
            });
            match session.request_file(&transfer_id) {
                Ok(QueueFileResult::Queued) => {
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                    with_status(shared, |s| {
                        s.file_transfer_phase = Some("receiving".into());
                    });
                }
                Ok(other) => {
                    with_status(shared, |s| {
                        s.file_transfer_phase = Some("failed".into());
                        s.last_error = Some(format!("auto request failed: {other:?}"));
                    });
                }
                Err(err) => {
                    with_status(shared, |s| {
                        s.file_transfer_phase = Some("failed".into());
                        s.last_error = Some(format!("auto request error: {err}"));
                    });
                }
            }
            Ok(())
        }
        InboundFileResult::Applied {
            transfer_id,
            file_name,
            path,
            size,
            sha256_hex,
        } => {
            let dir = with_status(shared, |s| PathBuf::from(s.file_save_dir.clone()));
            match file_save::finalize_part_file(&dir, &file_name, &path) {
                Ok(saved) => {
                    let saved_s = saved.display().to_string();
                    with_status(shared, |s| {
                        s.file_transfer_phase = Some("done".into());
                        s.last_file_transfer_id = Some(transfer_id);
                        s.last_file_name = Some(file_name.clone());
                        s.last_file_bytes = Some(size);
                        s.last_file_saved_path = Some(saved_s.clone());
                        s.file_bytes_received = Some(size);
                        s.file_bytes_total = Some(size);
                        s.last_sync_text = Some(format!(
                            "[file saved {file_name} {size}B sha256={sha256_hex}]"
                        ));
                        s.last_error = None;
                    });
                    println!("file_saved={saved_s} sha256={sha256_hex}");
                }
                Err(err) => {
                    let _ = fs::remove_file(&path);
                    with_status(shared, |s| {
                        s.file_transfer_phase = Some("failed".into());
                        s.last_file_transfer_id = Some(transfer_id);
                        s.last_file_name = Some(file_name);
                        s.last_error = Some(format!("file save: {err}"));
                    });
                }
            }
            Ok(())
        }
        InboundFileResult::Failed {
            transfer_id,
            message,
        } => {
            with_status(shared, |s| {
                mark_file_failed_if_current(s, &transfer_id, &message);
            });
            Ok(())
        }
        InboundFileResult::Chunk { .. } | InboundFileResult::StreamCompleted { .. } => Ok(()),
        InboundFileResult::BatchOffered { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    static BRIDGE_TEST_LOCK: Mutex<()> = Mutex::new(());
    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn batch_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("m590-hub-batch-{nanos}-{sequence}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn clipboard_file_list_uses_batch_for_multiple_roots_or_a_directory() {
        let root = batch_test_dir();
        let first = root.join("first.txt");
        let second = root.join("second.txt");
        let folder = root.join("folder");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        fs::create_dir(&folder).unwrap();

        assert!(!file_list_requires_batch(&[]));
        assert!(!file_list_requires_batch(std::slice::from_ref(&first)));
        assert!(file_list_requires_batch(&[first, second]));
        assert!(file_list_requires_batch(&[folder]));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_scan_is_stable_and_skips_nested_symlinks() {
        let root = batch_test_dir();
        let folder = root.join("folder");
        fs::create_dir_all(folder.join("empty")).unwrap();
        fs::write(folder.join("z.txt"), b"zzz").unwrap();
        fs::write(folder.join("a.txt"), b"a").unwrap();
        let loose = root.join("loose.bin");
        fs::write(&loose, b"loose").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&loose, folder.join("ignored-link")).unwrap();

        let prepared = scan_batch_paths(vec![
            loose.to_string_lossy().into_owned(),
            folder.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let paths: Vec<&str> = prepared
            .entries
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec![
                "folder",
                "folder/a.txt",
                "folder/empty",
                "folder/z.txt",
                "loose.bin"
            ]
        );
        assert_eq!(prepared.sources.len(), 3);
        assert!(paths.iter().all(|path| !path.contains("ignored-link")));
        assert_eq!(prepared.display_name, "2 items");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inbound_batch_stages_nested_files_and_publishes_as_one_tree() {
        let root = batch_test_dir();
        let save_dir = root.join("inbox");
        fs::create_dir_all(save_dir.join("folder")).unwrap();
        let entries = vec![
            BatchEntry::directory("dir-1", "folder").unwrap(),
            BatchEntry::directory("dir-2", "folder/empty").unwrap(),
            BatchEntry::file("file-1", "folder/nested/a.txt", 3, "").unwrap(),
            BatchEntry::file("file-2", "folder/zero.bin", 0, "").unwrap(),
        ];
        let mut batch = prepare_inbound_batch(
            "batch-test".into(),
            "folder".into(),
            entries,
            save_dir.clone(),
        )
        .unwrap();
        let first_part = batch.partial_dir.join("file-1.part");
        let second_part = batch.partial_dir.join("file-2.part");
        fs::write(&first_part, b"abc").unwrap();
        fs::write(&second_part, b"").unwrap();
        stage_completed_batch_file(
            &batch,
            &batch.files[0],
            "folder/nested/a.txt",
            &first_part,
            3,
        )
        .unwrap();
        stage_completed_batch_file(&batch, &batch.files[1], "folder/zero.bin", &second_part, 0)
            .unwrap();

        let published = commit_inbound_batch(&mut batch).unwrap();
        assert_eq!(published.file_name().unwrap(), "folder-1");
        assert_eq!(fs::read(published.join("nested/a.txt")).unwrap(), b"abc");
        assert_eq!(fs::metadata(published.join("zero.bin")).unwrap().len(), 0);
        assert!(published.join("empty").is_dir());
        assert!(!batch.staging_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dropping_uncommitted_batch_removes_parts_and_staging_tree() {
        let root = batch_test_dir();
        let save_dir = root.join("inbox");
        let entries = vec![BatchEntry::file("file-1", "a.txt", 4, "").unwrap()];
        let batch =
            prepare_inbound_batch("batch-clean".into(), "a.txt".into(), entries, save_dir).unwrap();
        let part = batch.partial_dir.join("file-1.part");
        let staging = batch.staging_dir.clone();
        fs::write(&part, b"part").unwrap();
        drop(batch);
        assert!(!part.exists());
        assert!(!staging.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn batch_json_paths_require_a_string_array() {
        assert_eq!(
            json_string_array(r#"{"paths":["a","b"]}"#, "paths").unwrap(),
            vec!["a", "b"]
        );
        assert!(json_string_array(r#"{"paths":"a"}"#, "paths").is_err());
        assert!(json_string_array(r#"{"paths":[1]}"#, "paths").is_err());
    }

    fn http_exchange(request: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_http(
                stream,
                crate::status::new_shared_status(),
                Arc::new(Mutex::new(None)),
                Arc::from(TEST_TOKEN),
            )
            .unwrap();
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(request.as_bytes()).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();
        response
    }

    fn request(method: &str, path: &str, headers: &str, body: &str) -> String {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn generated_hub_token_has_256_bits_of_hex() {
        let token = generate_hub_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn hub_control_api_ready_before_mdns_init_completes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        drop(listener);

        let ready = Arc::new(AtomicBool::new(false));
        let ready_flag = Arc::clone(&ready);
        let token = TEST_TOKEN.to_string();
        let addr_thread = addr.clone();
        let server = thread::spawn(move || {
            let _ = run_hub_with_token_on_ready(
                &addr_thread,
                token,
                Some(Box::new(move || {
                    ready_flag.store(true, Ordering::SeqCst);
                })),
            );
        });

        let deadline = Instant::now() + Duration::from_secs(2);
        while !ready.load(Ordering::SeqCst) {
            if Instant::now() > deadline {
                panic!("hub did not become ready within 2s");
            }
            thread::sleep(Duration::from_millis(10));
        }

        let mut client = TcpStream::connect(&addr).unwrap();
        let req = request(
            "GET",
            "/api/health",
            &format!("X-M590-Token: {TEST_TOKEN}\r\n"),
            "",
        );
        client.write_all(req.as_bytes()).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
        assert!(response.contains("\"ok\":true"), "{response}");
        let _ = server.thread().id();
    }

    #[test]
    fn session_loop_pause_is_work_aware() {
        assert_eq!(
            session_loop_pause(false, false),
            SessionLoopPause::Sleep(Duration::from_millis(50))
        );
        assert_eq!(
            session_loop_pause(true, false),
            SessionLoopPause::Sleep(Duration::from_millis(1))
        );
        assert_eq!(session_loop_pause(true, true), SessionLoopPause::Yield);
        assert_eq!(session_loop_pause(false, true), SessionLoopPause::Yield);
    }

    #[test]
    fn requested_virtual_receive_must_finish_before_replacement() {
        assert!(!active_virtual_receive_must_finish(false, false));
        assert!(active_virtual_receive_must_finish(true, false));
        assert!(!active_virtual_receive_must_finish(true, true));
    }

    #[test]
    fn clipboard_replacement_preserves_only_an_active_requested_stream() {
        assert_eq!(
            clipboard_replacement_disposition(false, false),
            ClipboardReplacementDisposition::ReleaseClipboardOffer
        );
        assert_eq!(
            clipboard_replacement_disposition(true, false),
            ClipboardReplacementDisposition::KeepActiveTransfer
        );
        assert_eq!(
            clipboard_replacement_disposition(true, true),
            ClipboardReplacementDisposition::ReleaseClipboardOffer
        );
    }

    #[test]
    fn replaced_virtual_receive_detaches_only_after_stream_completion() {
        assert!(!completed_replaced_virtual_receive_can_detach(false, false));
        assert!(!completed_replaced_virtual_receive_can_detach(false, true));
        assert!(!completed_replaced_virtual_receive_can_detach(true, false));
        assert!(completed_replaced_virtual_receive_can_detach(true, true));
    }

    #[test]
    fn linux_virtual_receive_waits_for_network_and_consumer_completion() {
        assert!(!linux_virtual_receive_must_finish(
            false, false, false, false
        ));
        assert!(linux_virtual_receive_must_finish(true, false, false, false));
        assert!(linux_virtual_receive_must_finish(true, true, false, true));
        assert!(linux_virtual_receive_must_finish(true, false, true, true));
        assert!(linux_virtual_receive_must_finish(true, true, true, false));
        assert!(!linux_virtual_receive_must_finish(true, true, true, true));
    }

    #[test]
    fn linux_replaced_mount_detaches_only_when_verified_and_consumed() {
        assert!(!linux_completed_replaced_virtual_receive_can_detach(
            false, false, false, false
        ));
        assert!(!linux_completed_replaced_virtual_receive_can_detach(
            true, false, true, true
        ));
        assert!(!linux_completed_replaced_virtual_receive_can_detach(
            false, true, true, true
        ));
        assert!(!linux_completed_replaced_virtual_receive_can_detach(
            true, true, false, true
        ));
        assert!(linux_completed_replaced_virtual_receive_can_detach(
            true, true, true, true
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_virtual_chunk_routing_backpressures_without_waiting() {
        let (bridge, producer) = VirtualFileBridge::with_capacity(1);
        let receive = LinuxVirtualReceive {
            transfer_id: "large-file".into(),
            bridge,
            producer,
            requested: true,
            completed: false,
            consumed: false,
            released: false,
            clipboard_replaced: false,
        };

        assert_eq!(
            try_push_linux_virtual_chunk("large-file", b"a", Some(&receive), None).unwrap(),
            LinuxVirtualChunkPush::Accepted
        );
        let started = Instant::now();
        assert_eq!(
            try_push_linux_virtual_chunk("large-file", b"b", Some(&receive), None).unwrap(),
            LinuxVirtualChunkPush::Backpressured
        );
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(
            try_push_linux_virtual_chunk("other", b"b", Some(&receive), None).unwrap(),
            LinuxVirtualChunkPush::NoReceiver
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_virtual_batch_preparation_is_lazy_and_schedules_requested_files() {
        let offer = DeferredVirtualBatchOffer {
            batch_id: "batch-1".into(),
            display_name: "batch".into(),
            entries: vec![
                BatchEntry::directory("dir-1", "folder").unwrap(),
                BatchEntry::file("file-1", "folder/one.bin", 3, "").unwrap(),
                BatchEntry::file("file-2", "two.bin", 4, "").unwrap(),
            ],
        };
        let (_tree, mut receive) = prepare_linux_virtual_batch_offer(&offer).unwrap();

        assert_eq!(receive.files.len(), 2);
        assert_eq!(receive.files[0].entry.relative_path, "folder/one.bin");
        assert_eq!(receive.files[1].entry.relative_path, "two.bin");
        assert!(receive
            .files
            .iter()
            .all(|file| file.bridge.take_event().is_none()));
        assert_eq!(receive.next_requested_index(), None);
        assert!(!receive.must_finish());

        receive.files[1].requested = true;
        assert_eq!(receive.next_requested_index(), Some(1));
        assert!(receive.must_finish());
        receive.files[1].completed = true;
        receive.files[1].consumed = true;
        assert!(receive.must_finish());
        receive.files[1].released = true;
        assert!(!receive.must_finish());
        assert!(!receive.is_complete());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_virtual_directory_only_batch_needs_no_stream() {
        let offer = DeferredVirtualBatchOffer {
            batch_id: "batch-empty-dir".into(),
            display_name: "empty folders".into(),
            entries: vec![
                BatchEntry::directory("dir-1", "empty").unwrap(),
                BatchEntry::directory("dir-2", "empty/nested").unwrap(),
            ],
        };
        let (_tree, receive) = prepare_linux_virtual_batch_offer(&offer).unwrap();

        assert!(receive.files.is_empty());
        assert!(receive.is_complete());
        assert!(!receive.must_finish());
        assert_eq!(receive.next_requested_index(), None);
    }

    #[test]
    fn completed_active_send_promotes_queued_offer_status() {
        let shared = crate::status::new_shared_status();
        with_status(&shared, |status| {
            status.file_transfer_phase = Some("sending".into());
            status.last_file_transfer_id = Some("active".into());
            status.last_file_name = Some("active.bin".into());
            status.last_file_bytes = Some(8);
            status.file_bytes_total = Some(8);
        });
        let mut queued = Some(QueuedFileStatus {
            summary: "queued summary".into(),
            transfer_id: "queued".into(),
            file_name: "queued.bin".into(),
            bytes: 12,
        });
        let payload =
            m590_core::FileCompletePayload::new(DeviceId::new("local"), "active", true, "ok")
                .unwrap();
        let resolutions =
            note_outbound_file_completes(&shared, &[Message::file_complete(payload)], &mut queued);

        assert_eq!(resolutions, vec![("active".into(), true)]);
        assert!(queued.is_none());
        let status = with_status(&shared, |status| status.clone());
        assert_eq!(status.file_transfer_phase.as_deref(), Some("sending"));
        assert_eq!(status.last_file_transfer_id.as_deref(), Some("queued"));
        assert_eq!(status.last_file_name.as_deref(), Some("queued.bin"));
        assert_eq!(status.file_bytes_total, Some(12));
    }

    #[test]
    fn stale_outbound_completion_does_not_promote_or_replace_current_status() {
        let shared = crate::status::new_shared_status();
        with_status(&shared, |status| {
            status.file_transfer_phase = Some("sending".into());
            status.last_file_transfer_id = Some("current".into());
            status.last_file_name = Some("current.bin".into());
        });
        let mut queued = Some(QueuedFileStatus {
            summary: "queued summary".into(),
            transfer_id: "queued".into(),
            file_name: "queued.bin".into(),
            bytes: 12,
        });
        let payload =
            m590_core::FileCompletePayload::new(DeviceId::new("local"), "stale", true, "ok")
                .unwrap();
        note_outbound_file_completes(&shared, &[Message::file_complete(payload)], &mut queued);

        assert_eq!(queued.as_ref().unwrap().transfer_id, "queued");
        let status = with_status(&shared, |status| status.clone());
        assert_eq!(status.last_file_transfer_id.as_deref(), Some("current"));
        assert_eq!(status.last_file_name.as_deref(), Some("current.bin"));
    }

    #[test]
    fn failed_deferred_offer_is_removed_from_status_queue() {
        let mut queued = Some(QueuedFileStatus {
            summary: "queued summary".into(),
            transfer_id: "queued".into(),
            file_name: "queued.bin".into(),
            bytes: 12,
        });

        assert!(!discard_queued_file_status(&mut queued, "other"));
        assert!(queued.is_some());
        assert!(discard_queued_file_status(&mut queued, "queued"));
        assert!(queued.is_none());
    }

    #[test]
    fn only_virtual_reader_failures_rearm_clipboard_file_offer() {
        assert!(file_failure_rearms_clipboard_offer(
            "virtual file reader closed"
        ));
        assert!(file_failure_rearms_clipboard_offer(
            "virtual file read timeout"
        ));
        assert!(!file_failure_rearms_clipboard_offer("clipboard replaced"));
        assert!(!file_failure_rearms_clipboard_offer(
            "replaced by a newer file offer"
        ));
    }

    #[test]
    fn stale_file_failure_does_not_replace_newer_offer_status() {
        let mut status = HubStatus {
            file_transfer_phase: Some("sending".into()),
            last_file_transfer_id: Some("new-offer".into()),
            last_file_name: Some("new.bin".into()),
            last_error: None,
            ..HubStatus::default()
        };

        assert!(!mark_file_failed_if_current(
            &mut status,
            "old-offer",
            "replaced by a newer file offer",
        ));
        assert_eq!(status.file_transfer_phase.as_deref(), Some("sending"));
        assert_eq!(status.last_file_transfer_id.as_deref(), Some("new-offer"));
        assert_eq!(status.last_file_name.as_deref(), Some("new.bin"));
        assert_eq!(status.last_error, None);
    }

    #[test]
    fn current_file_failure_is_still_reported() {
        let mut status = HubStatus {
            file_transfer_phase: Some("sending".into()),
            last_file_transfer_id: Some("current-offer".into()),
            last_error: None,
            ..HubStatus::default()
        };

        assert!(mark_file_failed_if_current(
            &mut status,
            "current-offer",
            "checksum mismatch",
        ));
        assert_eq!(status.file_transfer_phase.as_deref(), Some("failed"));
        assert_eq!(
            status.last_error.as_deref(),
            Some("file transfer failed: checksum mismatch")
        );
    }

    #[test]
    fn json_get_decodes_escaped_file_path() {
        assert_eq!(
            json_get(r#"{"path":"/tmp/line\nquote\"slash\\.txt"}"#, "path"),
            Some("/tmp/line\nquote\"slash\\.txt".into())
        );
    }

    #[test]
    fn oversized_content_length_is_rejected_before_body_read() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream).unwrap_err()
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let header = format!(
            "POST /api/send_file_bytes HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_HTTP_FILE_BODY_BYTES + 1
        );
        client.write_all(header.as_bytes()).unwrap();
        let err = server.join().unwrap();
        assert!(err.contains("http body too large"), "{err}");
    }

    #[test]
    fn hub_rejects_missing_token_and_untrusted_origin() {
        let no_token = http_exchange(request("GET", "/api/status", "", ""));
        assert!(no_token.starts_with("HTTP/1.1 401 "), "{no_token}");

        let evil = http_exchange(request(
            "GET",
            "/api/status",
            &format!("Origin: https://example.invalid\r\nX-M590-Token: {TEST_TOKEN}\r\n"),
            "",
        ));
        assert!(evil.starts_with("HTTP/1.1 403 "), "{evil}");
        assert!(!evil.contains("Access-Control-Allow-Origin"), "{evil}");
    }

    #[test]
    fn hub_accepts_authenticated_tauri_origin() {
        let response = http_exchange(request(
            "GET",
            "/api/status",
            &format!("Origin: tauri://localhost\r\nX-M590-Token: {TEST_TOKEN}\r\n"),
            "",
        ));
        assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
        assert!(
            response.contains("Access-Control-Allow-Origin: tauri://localhost"),
            "{response}"
        );
        assert!(!response.contains("Access-Control-Allow-Origin: *"));
    }

    #[test]
    fn file_bytes_http_body_accepts_more_than_one_mib() {
        use base64::Engine;

        let encoded = base64::engine::general_purpose::STANDARD.encode(vec![5u8; 1024 * 1024]);
        let body = format!("{{\"name\":\"one.bin\",\"data_base64\":\"{encoded}\"}}");
        assert!(body.len() > MAX_HTTP_JSON_BODY_BYTES);
        assert!(body.len() < MAX_HTTP_FILE_BODY_BYTES);
        let response = http_exchange(request(
            "POST",
            "/api/send_file_bytes",
            &format!("X-M590-Token: {TEST_TOKEN}\r\nContent-Type: application/json\r\n"),
            &body,
        ));
        assert!(response.starts_with("HTTP/1.1 400 "), "{response}");
        assert!(response.contains("not connected"), "{response}");
        assert!(!response.contains("http body too large"), "{response}");
    }

    #[test]
    fn pending_commands_reject_overwrite_and_disconnected_push() {
        clear_pending_commands();
        let shared = crate::status::new_shared_status();
        with_status(&shared, |status| status.phase = HubPhase::Connected);
        assert_eq!(push_text(&shared, "first".into()), Ok(()));
        assert_eq!(
            push_text(&shared, "second".into()),
            Err("text push already pending".into())
        );
        assert_eq!(
            PENDING_COMMANDS.lock().unwrap().text.as_deref(),
            Some("first")
        );
        clear_pending_commands();

        let root = batch_test_dir();
        let folder = root.join("folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.txt"), b"a").unwrap();
        assert_eq!(
            push_batch(&shared, vec![folder.to_string_lossy().into_owned()]),
            Ok(())
        );
        {
            let pending = PENDING_COMMANDS.lock().unwrap();
            let batch = pending.batch.as_ref().expect("queued batch");
            assert_eq!(batch.entries.len(), 2);
            assert_eq!(batch.sources.len(), 1);
        }
        assert_eq!(queue_batch_cancel(&shared), Ok(()));
        {
            let pending = PENDING_COMMANDS.lock().unwrap();
            assert!(pending.batch.is_none());
            assert!(pending.cancel_batch);
        }
        clear_pending_commands();
        let _ = fs::remove_dir_all(root);

        with_status(&shared, |status| status.phase = HubPhase::Idle);
        assert_eq!(
            push_text(&shared, "later".into()),
            Err("not connected".into())
        );
        assert!(PENDING_COMMANDS.lock().unwrap().text.is_none());
        assert_eq!(
            push_file_bytes(&shared, "later.bin".into(), "YQ==".into()),
            Err("not connected".into())
        );
        assert!(PENDING_COMMANDS.lock().unwrap().file_bytes.is_none());
    }

    #[test]
    fn initial_joiner_deadline_stops_reconnect_worker() {
        let _test_lock = BRIDGE_TEST_LOCK.lock().unwrap();
        let _transition = BRIDGE_TRANSITION.lock().unwrap();
        STOP_BRIDGE.store(false, Ordering::SeqCst);
        BRIDGE_STOPPING.store(false, Ordering::SeqCst);
        assert!(!BRIDGE_RUNNING.swap(true, Ordering::SeqCst));

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        drop(listener);
        let shared = Arc::new(Mutex::new(HubStatus::default()));
        with_status(&shared, |status| {
            status.auto_reconnect = true;
            status.phase = HubPhase::Pairing;
        });
        let discovery = Arc::new(Mutex::new(None));
        let started = Instant::now();

        run_with_reconnect_with_timeouts(
            shared.clone(),
            BridgeJob::Connect {
                code: "123456".into(),
                addr,
                device_id: "deadline-test".into(),
            },
            discovery,
            Duration::from_millis(250),
            Duration::from_millis(50),
        );

        let elapsed = started.elapsed();
        let status = with_status(&shared, |status| status.clone());
        assert!(elapsed >= Duration::from_millis(200), "elapsed={elapsed:?}");
        assert!(elapsed < Duration::from_secs(2), "elapsed={elapsed:?}");
        assert_eq!(status.phase, HubPhase::Error);
        assert!(
            status
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("pairing timeout (250ms)")),
            "{:?}",
            status.last_error
        );
        assert!(!BRIDGE_RUNNING.load(Ordering::SeqCst));
        assert!(!BRIDGE_STOPPING.load(Ordering::SeqCst));
        STOP_BRIDGE.store(false, Ordering::SeqCst);
    }

    #[test]
    fn wait_until_stopped_observes_async_worker_cleanup() {
        let running = Arc::new(AtomicBool::new(true));
        let stopping = Arc::new(AtomicBool::new(true));
        let running_worker = Arc::clone(&running);
        let stopping_worker = Arc::clone(&stopping);
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            running_worker.store(false, Ordering::SeqCst);
            stopping_worker.store(false, Ordering::SeqCst);
        });

        assert!(wait_until_stopped(
            &running,
            &stopping,
            Duration::from_millis(500)
        ));
        worker.join().unwrap();
    }

    #[test]
    fn wait_until_stopped_reports_timeout() {
        let running = AtomicBool::new(true);
        let stopping = AtomicBool::new(false);
        assert!(!wait_until_stopped(
            &running,
            &stopping,
            Duration::from_millis(20)
        ));
    }

    #[test]
    fn disconnect_completion_allows_first_reconnect_attempt() {
        let _test_lock = BRIDGE_TEST_LOCK.lock().unwrap();
        {
            let _transition = BRIDGE_TRANSITION.lock().unwrap();
            assert!(!BRIDGE_RUNNING.swap(true, Ordering::SeqCst));
            BRIDGE_STOPPING.store(false, Ordering::SeqCst);
            STOP_BRIDGE.store(false, Ordering::SeqCst);
        }
        let worker = thread::spawn(|| {
            while !STOP_BRIDGE.load(Ordering::SeqCst) {
                thread::yield_now();
            }
            thread::sleep(Duration::from_millis(30));
            BRIDGE_RUNNING.store(false, Ordering::SeqCst);
            BRIDGE_STOPPING.store(false, Ordering::SeqCst);
        });
        let shared = Arc::new(Mutex::new(HubStatus::default()));
        let discovery = Arc::new(Mutex::new(None));

        stop_bridge(&shared, &discovery).unwrap();
        worker.join().unwrap();
        {
            let _transition = BRIDGE_TRANSITION.lock().unwrap();
            assert_eq!(claim_bridge(), Ok(()));
            BRIDGE_RUNNING.store(false, Ordering::SeqCst);
            STOP_BRIDGE.store(false, Ordering::SeqCst);
        }
    }

    #[test]
    fn reconnect_backoff_sequence_is_unchanged() {
        let actual = (1..=7).map(reconnect_delay_secs).collect::<Vec<_>>();
        assert_eq!(actual, vec![1, 2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn reconnect_decision_allows_post_connection_pairing_timeout() {
        let timeout = "pairing timeout (30s): peer silent";
        assert!(should_stop_reconnecting(timeout, false));
        assert!(!should_stop_reconnecting(timeout, true));
        assert!(should_stop_reconnecting("pairing code mismatch", true));
        assert!(!should_stop_reconnecting("tcp connection refused", true));
    }
}
