//! Localhost HTTP control API for the operable UI shell.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use m590_clipboard::{ClipboardService, PlatformClipboard};
use m590_core::{
    ConnectionState, DeviceId, InboundClipboardResult, InboundFileResult, Message,
    QueueClipboardResult, QueueFileResult, Session, SessionEvent,
    DEFAULT_HEARTBEAT_MISS_THRESHOLD, MAX_FILE_BYTES, MAX_MEMORY_FILE_BYTES,
};
use m590_net::{accept_framed, connect_framed, listen_on, TcpFrameStream};

use crate::config;
use crate::discovery::DiscoveryHandle;
use crate::file_save;
use crate::status::{persist_status_config, with_status, HubPhase, SharedStatus};

static STOP_BRIDGE: AtomicBool = AtomicBool::new(false);
static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);

type SharedDiscovery = Arc<Option<Arc<DiscoveryHandle>>>;

pub fn run_hub(api_addr: &str) -> Result<(), String> {
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

    let device_id = with_status(&shared, |s| s.device_id.clone());
    let discovery: SharedDiscovery = match DiscoveryHandle::start(device_id) {
        Ok(h) => {
            println!("mdns_browse=on type={}", crate::discovery::SERVICE_TYPE);
            Arc::new(Some(h))
        }
        Err(err) => {
            eprintln!("mdns_browse=off error={err}");
            Arc::new(None)
        }
    };

    let listener = TcpListener::bind(api_addr).map_err(|e| format!("bind hub api failed: {e}"))?;
    println!("hub_api=http://{api_addr}");
    println!("hub_status=ready (UI can open operable shell and point API to this address)");
    println!(
        "endpoints=GET /api/status /api/config /api/discover POST /api/discover/refresh /api/listen /api/connect /api/push /api/send_file /api/disconnect /api/config"
    );
    let cfg_path = config::default_config_path();
    println!("config_path={}", cfg_path.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let shared = Arc::clone(&shared);
                let discovery = Arc::clone(&discovery);
                thread::spawn(move || {
                    if let Err(err) = handle_http(stream, shared, discovery) {
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
) -> Result<(), String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let (method, path, body) = read_http_request(&mut stream)?;

    if method == "OPTIONS" {
        return write_response(&mut stream, 204, "text/plain", "");
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/api/health") => {
            write_response(&mut stream, 200, "application/json", "{\"ok\":true}")
        }
        ("GET", "/api/status") => {
            let json = with_status(&shared, |s| s.to_json());
            write_response(&mut stream, 200, "application/json", &json)
        }
        ("GET", "/api/config") => {
            let json = with_status(&shared, |s| s.snapshot_config().to_json());
            write_response(&mut stream, 200, "application/json", &json)
        }
        ("GET", "/api/discover") => {
            let json = match discovery.as_ref() {
                Some(d) => d.to_json(),
                None => {
                    "{\"service_type\":\"_m590bridge._tcp.local.\",\"advertising\":false,\"peers\":[],\"error\":\"mdns unavailable\"}".into()
                }
            };
            write_response(&mut stream, 200, "application/json", &json)
        }
        ("POST", "/api/discover/refresh") => match discovery.as_ref() {
            Some(d) => match d.refresh() {
                Ok(()) => write_response(
                    &mut stream,
                    200,
                    "application/json",
                    &d.to_json(),
                ),
                Err(err) => write_json_err(&mut stream, &err),
            },
            None => write_json_err(&mut stream, "mdns unavailable"),
        },
        ("POST", "/api/config") => match apply_config_update(&shared, &body) {
            Ok(json) => write_response(&mut stream, 200, "application/json", &json),
            Err(err) => write_json_err(&mut stream, &err),
        },
        ("POST", "/api/listen") => {
            let code = resolve_pairing_code(&shared, &body);
            let port = json_get(&body, "port")
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(|| with_status(&shared, |s| s.listen_port));
            let device_id = json_get(&body, "device_id").filter(|s| !s.is_empty());
            match start_listen(shared, code, port, device_id, discovery) {
                Ok(()) => write_response(&mut stream, 200, "application/json", "{\"ok\":true}"),
                Err(err) => write_json_err(&mut stream, &err),
            }
        }
        ("POST", "/api/connect") => {
            let code = resolve_pairing_code(&shared, &body);
            let addr = json_get(&body, "addr")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    with_status(&shared, |s| s.connect_addr.clone().unwrap_or_default())
                });
            let device_id = json_get(&body, "device_id").filter(|s| !s.is_empty());
            match start_connect(shared, code, addr, device_id, discovery) {
                Ok(()) => write_response(&mut stream, 200, "application/json", "{\"ok\":true}"),
                Err(err) => write_json_err(&mut stream, &err),
            }
        }
        ("POST", "/api/push") => {
            let text = json_get(&body, "text").unwrap_or_default();
            match push_text(&shared, text) {
                Ok(()) => write_response(&mut stream, 200, "application/json", "{\"ok\":true}"),
                Err(err) => write_json_err(&mut stream, &err),
            }
        }
        ("POST", "/api/send_file") => {
            let file_path = json_get(&body, "path").unwrap_or_default();
            match push_file(&shared, file_path) {
                Ok(()) => write_response(&mut stream, 200, "application/json", "{\"ok\":true}"),
                Err(err) => write_json_err(&mut stream, &err),
            }
        }
        ("POST", "/api/send_file_bytes") => {
            let name = json_get(&body, "name").unwrap_or_default();
            let data_b64 = json_get(&body, "data_base64").unwrap_or_default();
            match push_file_bytes(&shared, name, data_b64) {
                Ok(()) => write_response(&mut stream, 200, "application/json", "{\"ok\":true}"),
                Err(err) => write_json_err(&mut stream, &err),
            }
        }
        ("POST", "/api/disconnect") => {
            STOP_BRIDGE.store(true, Ordering::SeqCst);
            if let Some(d) = discovery.as_ref() {
                d.stop_advertise();
            }
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
            write_response(&mut stream, 200, "application/json", "{\"ok\":true}")
        }
        _ => write_response(
            &mut stream,
            404,
            "application/json",
            "{\"error\":\"not found\"}",
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


fn read_http_request(stream: &mut TcpStream) -> Result<(String, String, String), String> {
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
        if data.len() > 64 * 1024 {
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

    let mut content_length = 0usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    let mut body = data[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
        if body.len() > 1024 * 1024 {
            return Err("http body too large".into());
        }
    }
    if content_length > 0 {
        body.truncate(content_length);
    }
    let body = String::from_utf8_lossy(&body).trim_end_matches('\0').to_string();
    Ok((method, path, body))
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

fn write_json_err(stream: &mut TcpStream, err: &str) -> Result<(), String> {
    let body = format!(
        "{{\"ok\":false,\"error\":\"{}\"}}",
        err.replace('\\', "\\\\").replace('"', "\\\"")
    );
    write_response(stream, 400, "application/json", &body)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\n\
Content-Type: {content_type}; charset=utf-8\r\n\
Content-Length: {len}\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type\r\n\
Connection: close\r\n\
\r\n\
{body}",
        len = body.as_bytes().len(),
    );
    stream
        .write_all(resp.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

/// Very small JSON string field extractor: "key":"value" or "key": 123
fn json_get(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let idx = body.find(&pat)?;
    let after = &body[idx + pat.len()..];
    let after = after.trim_start().trim_start_matches(':').trim_start();
    if after.starts_with("null") {
        return None;
    }
    if let Some(rest) = after.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
                other => out.push(other),
            }
        }
        Some(out)
    } else {
        let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if num.is_empty() {
            None
        } else {
            Some(num)
        }
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

    if let Some(d) = discovery.as_ref() {
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
    if let Some(d) = discovery.as_ref() {
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
                    s.phase = if attempt == 0 {
                        HubPhase::WaitingPeer
                    } else {
                        HubPhase::WaitingPeer
                    };
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
            } => connect_worker(shared.clone(), code.clone(), addr.clone(), device_id.clone()),
        };

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
    if let Some(d) = discovery.as_ref() {
        d.stop_advertise();
    }
    BRIDGE_RUNNING.store(false, Ordering::SeqCst);
}

fn humanize_bridge_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if lower.contains("pairing rejected") || lower.contains("pairing code mismatch") {
        return format!(
            "配对码错误或已过期（两端请使用同一 6 位码）。详情：{err}"
        );
    }
    if lower.contains("pairing timeout") {
        return err.to_string();
    }
    if lower.contains("unexpected peer device id") {
        return format!(
            "两端 device_id 冲突或异常（设置里改成本机唯一名称后重试）。详情：{err}"
        );
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
    PENDING_PUSH.lock().expect("push lock").replace(text);
    let phase = with_status(shared, |s| s.phase);
    if phase != HubPhase::Connected {
        return Err("not connected".into());
    }
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
    PENDING_FILE.lock().expect("file lock").replace(path);
    let phase = with_status(shared, |s| s.phase);
    if phase != HubPhase::Connected {
        return Err("not connected".into());
    }
    Ok(())
}

static PENDING_PUSH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static PENDING_FILE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static PENDING_FILE_BYTES: std::sync::Mutex<Option<(String, Vec<u8>)>> =
    std::sync::Mutex::new(None);

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
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64.trim())
        .map_err(|e| format!("base64 decode: {e}"))?;
    if data.len() > MAX_MEMORY_FILE_BYTES {
        return Err(format!(
            "file too large for bytes API: {}B > limit {MAX_MEMORY_FILE_BYTES}B (use /api/send_file path)",
            data.len()
        ));
    }
    PENDING_FILE_BYTES
        .lock()
        .expect("file bytes lock")
        .replace((name, data));
    let phase = with_status(shared, |s| s.phase);
    if phase != HubPhase::Connected {
        return Err("not connected".into());
    }
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
    listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
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

        if let Some(text) = PENDING_PUSH.lock().expect("push").take() {
            content_seq += 1;
            let cid = format!("ui-push-{}-{content_seq}", std::process::id());
            if let Ok(QueueClipboardResult::Queued) = session.queue_clipboard_text(cid, text.clone())
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

        if let Some(path) = PENDING_FILE.lock().expect("file").take() {
            match offer_local_file(session, &mut content_seq, &path) {
                Ok((summary, transfer_id, file_name, bytes)) => {
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                    mark_file_sending(&shared, summary, transfer_id, file_name, bytes);
                }
                Err(err) => {
                    with_status(&shared, |s| {
                        s.file_transfer_phase = Some("failed".into());
                        s.last_error = Some(err);
                    });
                }
            }
        }

        if let Some((name, data)) = PENDING_FILE_BYTES.lock().expect("file bytes").take() {
            match offer_file_bytes(session, &mut content_seq, name, data) {
                Ok((summary, transfer_id, file_name, bytes)) => {
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                    mark_file_sending(&shared, summary, transfer_id, file_name, bytes);
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
            session
                .pump_outbound_file()
                .map_err(|e| e.to_string())?;
            let outbox = session.take_outbox();
            conn.send_all(outbox.iter())
                .map_err(|e| e.to_string())?;
            note_outbound_file_completes(&shared, &outbox);
            if let Some((tid, sent, total)) = session.outbound_file_progress() {
                with_status(&shared, |s| {
                    if s.last_file_transfer_id.as_deref() == Some(tid.as_str())
                        || s.file_transfer_phase.as_deref() == Some("sending")
                    {
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
                    session
                        .handle(SessionEvent::Message(msg))
                        .map_err(|e| e.to_string())?;
                    last_peer_rx = Instant::now();
                    let outbox = session.take_outbox();
                    conn.send_all(outbox.iter())
                        .map_err(|e| e.to_string())?;
                    note_outbound_file_completes(&shared, &outbox);
                    match session.take_inbound_clipboard() {
                        Some(InboundClipboardResult::Applied { content_id, text }) => {
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
                            let summary = format!(
                                "[image {width}x{height} {}B {encoding:?}]",
                                data.len()
                            );
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
                                                s.last_error = Some(format!(
                                                    "clipboard_write_image: {err}"
                                                ));
                                            });
                                        }
                                    }
                                    Err(err) => {
                                        with_status(&shared, |s| {
                                            s.last_error =
                                                Some(format!("image_decode: {err}"));
                                        });
                                    }
                                }
                            }
                        }
                        Some(InboundClipboardResult::DuplicateContentId) | None => {}
                    }

                    if let Some(file_event) = session.take_inbound_file() {
                        handle_inbound_file(&shared, session, conn, file_event)?;
                    } else if let Some((tid, got, total)) = session.inbound_file_progress() {
                        with_status(&shared, |s| {
                            s.file_transfer_phase = Some("receiving".into());
                            s.last_file_transfer_id = Some(tid);
                            s.file_bytes_received = Some(got);
                            s.file_bytes_total = Some(total);
                        });
                    }
                }
                Ok(None) => break,
                Err(m590_net::TcpError::Disconnected) => return Err("peer disconnected".into()),
                Err(err) => return Err(err.to_string()),
            }
        }

        if let Some(clip) = clipboard.as_mut() {
            let auto = with_status(&shared, |s| s.auto_sync);
            if auto {
                // File-manager copies expose text/uri-list (file_list), not plain text/image.
                if let Ok(Some(paths)) = clip.poll_file_list_change() {
                    // Images: keep bitmap clipboard path (Word/paint paste).
                    let mut handled = false;
                    if let Ok(Some(image)) = m590_clipboard::image_from_paths(&paths) {
                        content_seq += 1;
                        let cid =
                            format!("ui-clip-imgfiles-{}-{content_seq}", std::process::id());
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
                                    mark_file_sending(
                                        &shared,
                                        summary,
                                        transfer_id,
                                        file_name,
                                        bytes,
                                    );
                                    clip.adopt_text_baseline();
                                }
                                Err(err) => {
                                    with_status(&shared, |s| {
                                        s.last_error = Some(format!("file_list skip: {err}"));
                                    });
                                }
                            }
                        }
                    }
                }
                if let Ok(Some(text)) = clip.poll_text_change() {
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
                        match offer_local_file(
                            session,
                            &mut content_seq,
                            &path.to_string_lossy(),
                        ) {
                            Ok((summary, transfer_id, file_name, bytes)) => {
                                conn.send_all(session.take_outbox().iter())
                                    .map_err(|e| e.to_string())?;
                                mark_file_sending(
                                    &shared,
                                    summary,
                                    transfer_id,
                                    file_name,
                                    bytes,
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

        thread::sleep(Duration::from_millis(50));
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

/// Sender side: after FileRequest is answered, outbox contains FileComplete but status
/// used to stay on `sending` / 0%. Mirror complete into hub status for UI progress.
fn note_outbound_file_completes(shared: &SharedStatus, outbox: &[Message]) {
    for msg in outbox {
        let Message::FileComplete(payload) = msg else {
            continue;
        };
        with_status(shared, |s| {
            let matches_current = s
                .last_file_transfer_id
                .as_deref()
                .is_some_and(|id| id == payload.transfer_id);
            let sending = s.file_transfer_phase.as_deref() == Some("sending");
            if !(matches_current || sending) {
                return;
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
        });
    }
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
                s.file_transfer_phase = Some("failed".into());
                s.last_file_transfer_id = Some(transfer_id);
                s.last_error = Some(format!("file transfer failed: {message}"));
            });
            Ok(())
        }
    }
}
