//! Localhost HTTP control API for the operable UI shell.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use m590_clipboard::{ClipboardService, PlatformClipboard};
use m590_core::{
    ConnectionState, DeviceId, InboundClipboardResult, InboundFileResult, Message,
    QueueClipboardResult, QueueFileResult, Session, SessionEvent, DEFAULT_HEARTBEAT_MISS_THRESHOLD,
    MAX_FILE_BYTES, MAX_MEMORY_FILE_BYTES,
};
use m590_net::{accept_framed, connect_framed, listen_on, TcpFrameStream};

use crate::config;
use crate::discovery::DiscoveryHandle;
#[cfg(not(target_os = "windows"))]
use crate::file_save;
use crate::status::{persist_status_config, with_status, HubPhase, HubStatus, SharedStatus};
#[cfg(target_os = "windows")]
use crate::virtual_file_bridge::{BridgeEvent, PipeProducer, VirtualFileBridge};
#[cfg(target_os = "windows")]
use crate::windows_virtual_file_manager::{ManagerEvent, WindowsVirtualFileManager};

static STOP_BRIDGE: AtomicBool = AtomicBool::new(false);
static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);

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

#[cfg(target_os = "windows")]
struct WindowsVirtualReceive {
    transfer_id: String,
    bridge: VirtualFileBridge,
    producer: PipeProducer,
    requested: bool,
    completed: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct DeferredWindowsVirtualOffer {
    transfer_id: String,
    file_name: String,
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedFileStatus {
    summary: String,
    transfer_id: String,
    file_name: String,
    bytes: u64,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn active_virtual_receive_must_finish(requested: bool, completed: bool) -> bool {
    requested && !completed
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
        "endpoints=GET /api/status /api/config /api/discover POST /api/discover/refresh /api/listen /api/connect /api/push /api/send_file /api/disconnect /api/config"
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
        ("POST", "/api/disconnect") => {
            STOP_BRIDGE.store(true, Ordering::SeqCst);
            {
                let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
                with_status(&shared, |s| {
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
            if let Some(d) = discovery_handle(&discovery).as_ref() {
                d.stop_advertise();
            }
            write_response(
                &mut stream,
                200,
                "application/json",
                "{\"ok\":true}",
                cors_origin,
            )
        }
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

fn start_listen(
    shared: SharedStatus,
    mut code: String,
    port: u16,
    device_id: Option<String>,
    discovery: SharedDiscovery,
) -> Result<(), String> {
    code = normalize_pairing_code(&code);
    if code.is_empty() {
        // Last resort: generate a 6-digit code so host can still start.
        code = format!("{:06}", (std::process::id() % 900_000) + 100_000);
    }
    if BRIDGE_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("bridge already running; disconnect first".into());
    }
    STOP_BRIDGE.store(false, Ordering::SeqCst);
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
    code = normalize_pairing_code(&code);
    let addr = addr.trim().to_string();
    if code.is_empty() {
        return Err("code required".into());
    }
    if addr.is_empty() {
        return Err("addr required".into());
    }
    if BRIDGE_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("bridge already running; disconnect first".into());
    }
    STOP_BRIDGE.store(false, Ordering::SeqCst);
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
    let mut attempt: u32 = 0;
    loop {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
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
            } => listen_worker(shared.clone(), code.clone(), *port, device_id.clone()),
            BridgeJob::Connect {
                code,
                addr,
                device_id,
            } => connect_worker(
                shared.clone(),
                code.clone(),
                addr.clone(),
                device_id.clone(),
            ),
        };
        {
            let mut pending = PENDING_COMMANDS.lock().expect("pending commands lock");
            with_status(&shared, |s| {
                if s.phase == HubPhase::Connected {
                    s.phase = HubPhase::Idle;
                    s.connection = Some(ConnectionState::Disconnected);
                    s.peer_device = None;
                }
            });
            pending.clear();
        }

        if STOP_BRIDGE.load(Ordering::SeqCst) {
            with_status(&shared, |s| {
                s.phase = HubPhase::Idle;
                s.connection = Some(ConnectionState::Disconnected);
                s.peer_device = None;
                s.role = None;
                s.reconnect_attempt = 0;
                s.last_error = None;
            });
            break;
        }

        match result {
            Ok(()) => {
                // Clean stop from worker (e.g. accept loop saw STOP) or rare clean exit.
                with_status(&shared, |s| {
                    if s.phase != HubPhase::Idle {
                        s.phase = HubPhase::Idle;
                        s.connection = Some(ConnectionState::Disconnected);
                        s.peer_device = None;
                        s.role = None;
                    }
                    s.reconnect_attempt = 0;
                });
                break;
            }
            Err(err) => {
                let friendly = humanize_bridge_error(&err);
                // Version skew / bad pair code / timeout cannot self-heal by reconnecting.
                if is_protocol_mismatch(&err) || is_non_retriable_pair_error(&err) {
                    with_status(&shared, |s| {
                        s.phase = HubPhase::Error;
                        s.last_error = Some(friendly);
                        s.connection = Some(ConnectionState::Disconnected);
                        s.peer_device = None;
                        s.reconnect_attempt = 0;
                    });
                    break;
                }
                let auto = with_status(&shared, |s| s.auto_reconnect);
                if !auto {
                    with_status(&shared, |s| {
                        s.phase = HubPhase::Error;
                        s.last_error = Some(friendly);
                        s.connection = Some(ConnectionState::Disconnected);
                        s.peer_device = None;
                        s.reconnect_attempt = 0;
                    });
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
                if !sleep_interruptible(Duration::from_secs(delay_secs)) {
                    with_status(&shared, |s| {
                        s.phase = HubPhase::Idle;
                        s.connection = Some(ConnectionState::Disconnected);
                        s.peer_device = None;
                        s.role = None;
                        s.reconnect_attempt = 0;
                        s.last_error = None;
                    });
                    break;
                }
            }
        }
    }
    if let Some(d) = discovery_handle(&discovery).as_ref() {
        d.stop_advertise();
    }
    BRIDGE_RUNNING.store(false, Ordering::SeqCst);
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

/// Failures that will not heal by blind reconnect (wrong code, timeout, id clash).
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
    let shift = attempt.saturating_sub(1).min(4);
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
    if pending.file_path.is_some() {
        return Err("file send already pending".into());
    }
    pending.file_path = Some(path);
    Ok(())
}

struct PendingCommands {
    text: Option<String>,
    file_path: Option<String>,
    file_bytes: Option<(String, Vec<u8>)>,
}

impl PendingCommands {
    const fn new() -> Self {
        Self {
            text: None,
            file_path: None,
            file_bytes: None,
        }
    }

    fn clear(&mut self) {
        self.text = None;
        self.file_path = None;
        self.file_bytes = None;
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
    if pending.file_bytes.is_some() {
        return Err("file bytes send already pending".into());
    }
    pending.file_bytes = Some((name, data));
    Ok(())
}

fn listen_worker(
    shared: SharedStatus,
    code: String,
    port: u16,
    device_id: String,
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
    run_session_loop(shared, &mut session, &mut conn)
}

fn connect_worker(
    shared: SharedStatus,
    code: String,
    addr: String,
    device_id: String,
) -> Result<(), String> {
    let mut conn = connect_framed(&addr).map_err(|e| e.to_string())?;
    let mut session = Session::new(DeviceId::new(device_id)).map_err(|e| e.to_string())?;
    session
        .handle(SessionEvent::StartPairing {
            expected_code: code,
        })
        .map_err(|e| e.to_string())?;
    conn.send_all(session.take_outbox().iter())
        .map_err(|e| e.to_string())?;
    run_session_loop(shared, &mut session, &mut conn)
}

fn run_session_loop(
    shared: SharedStatus,
    session: &mut Session,
    conn: &mut TcpFrameStream,
) -> Result<(), String> {
    let mut clipboard = PlatformClipboard::open().ok();
    let mut last_heartbeat = Instant::now();
    let mut last_peer_rx = Instant::now();
    let mut content_seq = 0u64;
    let mut latest_clipboard_file_offer_id: Option<String> = None;
    let mut queued_file_status: Option<QueuedFileStatus> = None;
    #[cfg(target_os = "windows")]
    let ole_manager = WindowsVirtualFileManager::start().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let mut virtual_receive: Option<WindowsVirtualReceive> = None;
    #[cfg(target_os = "windows")]
    let mut deferred_virtual_offer: Option<DeferredWindowsVirtualOffer> = None;
    const PAIRING_TIMEOUT: Duration = Duration::from_secs(30);
    let pairing_started = Instant::now();

    while session.state() != ConnectionState::Connected {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
            let _ = session.handle(SessionEvent::Disconnect);
            return Ok(());
        }
        if pairing_started.elapsed() > PAIRING_TIMEOUT {
            let _ = session.handle(SessionEvent::Disconnect);
            return Err(format!(
                "pairing timeout ({}s): 请确认对端已开始等待/连接、配对码一致、防火墙放行端口",
                PAIRING_TIMEOUT.as_secs()
            ));
        }
        conn.set_read_timeout(Some(Duration::from_millis(200)))
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

        // Stream more file chunks without starving the rest of the loop forever.
        if session.has_pending_outbound_file() {
            session.pump_outbound_file().map_err(|e| e.to_string())?;
            file_progressed = true;
            let outbox = session.take_outbox();
            conn.send_all(outbox.iter()).map_err(|e| e.to_string())?;
            let resolutions =
                note_outbound_file_completes(&shared, &outbox, &mut queued_file_status);
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
            match conn.try_recv() {
                Ok(Some(msg)) => {
                    file_progressed |=
                        matches!(msg, Message::FileChunk(_) | Message::FileComplete(_));
                    session
                        .handle(SessionEvent::Message(msg))
                        .map_err(|e| e.to_string())?;
                    last_peer_rx = Instant::now();
                    let outbox = session.take_outbox();
                    conn.send_all(outbox.iter()).map_err(|e| e.to_string())?;
                    let resolutions =
                        note_outbound_file_completes(&shared, &outbox, &mut queued_file_status);
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
                        {
                            if let Some(current) = virtual_receive.as_mut() {
                                if let InboundFileResult::Chunk { transfer_id, data } = &file_event
                                {
                                    if current.transfer_id == *transfer_id {
                                        if let Err(err) = current.producer.push(data) {
                                            with_status(&shared, |s| {
                                                s.last_error =
                                                    Some(format!("virtual file stream: {err}"));
                                            });
                                        }
                                    }
                                }
                            }
                            match &file_event {
                                InboundFileResult::Offered {
                                    transfer_id,
                                    file_name,
                                    size,
                                } => {
                                    let next = DeferredWindowsVirtualOffer {
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
                                        });
                                    if must_defer {
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
                                        virtual_receive = Some(publish_windows_virtual_offer(
                                            &ole_manager,
                                            &next,
                                        )?);
                                        mark_windows_virtual_offer(&shared, &next);
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
                            let can_promote = virtual_receive
                                .as_ref()
                                .map(|current| current.completed)
                                .unwrap_or(true);
                            if can_promote {
                                if let Some(next) = deferred_virtual_offer.take() {
                                    virtual_receive.take();
                                    virtual_receive =
                                        Some(publish_windows_virtual_offer(&ole_manager, &next)?);
                                    mark_windows_virtual_offer(&shared, &next);
                                }
                            }
                        }
                        #[cfg(not(target_os = "windows"))]
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
                        #[cfg(not(target_os = "windows"))]
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
                Ok(None) => break,
                Err(m590_net::TcpError::Disconnected) => return Err("peer disconnected".into()),
                Err(err) => return Err(err.to_string()),
            }
        }

        #[cfg(target_os = "windows")]
        {
            let mut promote_deferred = false;
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
                        BridgeEvent::Cancel(reason) => {
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
                if keep_current {
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
                                current.producer.fail("clipboard replaced");
                                if !current.completed {
                                    let transfer_id = current.transfer_id.clone();
                                    session
                                        .cancel_file(transfer_id, "clipboard replaced")
                                        .map_err(|e| e.to_string())?;
                                    conn.send_all(session.take_outbox().iter())
                                        .map_err(|e| e.to_string())?;
                                }
                                keep_current = false;
                                discard_deferred = true;
                                latest_clipboard_file_offer_id = None;
                            }
                        }
                    }
                }
                if keep_current {
                    virtual_receive = Some(current);
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
            } else if promote_deferred {
                if let Some(next) = deferred_virtual_offer.take() {
                    virtual_receive = Some(publish_windows_virtual_offer(&ole_manager, &next)?);
                    mark_windows_virtual_offer(&shared, &next);
                }
            }
        }

        if let Some(clip) = clipboard.as_mut() {
            let auto = with_status(&shared, |s| s.auto_sync);
            #[cfg(target_os = "windows")]
            let virtual_clipboard_active = virtual_receive.is_some();
            #[cfg(not(target_os = "windows"))]
            let virtual_clipboard_active = false;
            if auto && !virtual_clipboard_active {
                // File-manager copies expose text/uri-list (file_list), not plain text/image.
                if let Ok(Some(paths)) = clip.poll_file_list_change() {
                    latest_clipboard_file_offer_id = None;
                    // Images: keep bitmap clipboard path (Word/paint paste).
                    let mut handled = false;
                    if let Ok(Some(image)) = m590_clipboard::image_from_paths(&paths) {
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
                    // Non-image regular files: V2 file offer (first path only, streamed ≤ MAX_FILE_BYTES).
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
                    } else if let Some(path) = m590_clipboard::regular_file_from_text(&text) {
                        // Linux often exposes a copied file as plain path text only.
                        match offer_local_file(session, &mut content_seq, &path.to_string_lossy()) {
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
                            }
                            Err(err) => {
                                with_status(&shared, |s| {
                                    s.last_error = Some(format!("path text skip: {err}"));
                                });
                            }
                        }
                    } else {
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

        match session_loop_pause(session.has_active_file_transfer(), file_progressed) {
            SessionLoopPause::Yield => thread::yield_now(),
            SessionLoopPause::Sleep(delay) => thread::sleep(delay),
        }
    }
}

fn mark_file_sending(
    shared: &SharedStatus,
    summary: String,
    transfer_id: String,
    file_name: String,
    bytes: u64,
) {
    with_status(shared, |s| {
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
    offer: &DeferredWindowsVirtualOffer,
) -> Result<WindowsVirtualReceive, String> {
    let (bridge, producer) = VirtualFileBridge::new();
    let file = bridge
        .virtual_file(offer.file_name.clone(), offer.size)
        .map_err(|e| e.to_string())?;
    manager.publish(file)?;
    Ok(WindowsVirtualReceive {
        transfer_id: offer.transfer_id.clone(),
        bridge,
        producer,
        requested: false,
        completed: false,
    })
}

#[cfg(target_os = "windows")]
fn mark_windows_virtual_offer(shared: &SharedStatus, offer: &DeferredWindowsVirtualOffer) {
    with_status(shared, |s| {
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

#[cfg(not(target_os = "windows"))]
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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
}
