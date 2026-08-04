//! Localhost HTTP control API for the operable UI shell.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use m590_clipboard::{ClipboardService, PlatformClipboard};
use m590_core::{
    ConnectionState, DeviceId, InboundClipboardResult, QueueClipboardResult, Session,
    SessionEvent, DEFAULT_HEARTBEAT_MISS_THRESHOLD,
};
use m590_net::{accept_framed, connect_framed, listen_on, TcpFrameStream};

use crate::config;
use crate::status::{persist_status_config, with_status, HubPhase, SharedStatus};

static STOP_BRIDGE: AtomicBool = AtomicBool::new(false);
static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn run_hub(api_addr: &str) -> Result<(), String> {
    let shared = crate::status::new_shared_status();
    with_status(&shared, |s| {
        s.hub_api = Some(format!("http://{api_addr}"));
        s.phase = HubPhase::Idle;
    });

    let listener = TcpListener::bind(api_addr).map_err(|e| format!("bind hub api failed: {e}"))?;
    println!("hub_api=http://{api_addr}");
    println!("hub_status=ready (UI can open operable shell and point API to this address)");
    println!(
        "endpoints=GET /api/status /api/config POST /api/listen /api/connect /api/push /api/disconnect /api/config"
    );
    let cfg_path = config::default_config_path();
    println!("config_path={}", cfg_path.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let shared = Arc::clone(&shared);
                thread::spawn(move || {
                    if let Err(err) = handle_http(stream, shared) {
                        eprintln!("hub_http_error={err}");
                    }
                });
            }
            Err(err) => eprintln!("hub_accept_error={err}"),
        }
    }
    Ok(())
}

fn handle_http(mut stream: TcpStream, shared: SharedStatus) -> Result<(), String> {
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
            match start_listen(shared, code, port, device_id) {
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
            match start_connect(shared, code, addr, device_id) {
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
        ("POST", "/api/disconnect") => {
            STOP_BRIDGE.store(true, Ordering::SeqCst);
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

    thread::spawn(move || {
        run_with_reconnect(shared, BridgeJob::Listen { code, port, device_id });
    });
    Ok(())
}

fn start_connect(
    shared: SharedStatus,
    mut code: String,
    addr: String,
    device_id: Option<String>,
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

    thread::spawn(move || {
        run_with_reconnect(
            shared,
            BridgeJob::Connect {
                code,
                addr,
                device_id,
            },
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

fn run_with_reconnect(shared: SharedStatus, job: BridgeJob) {
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
                let auto = with_status(&shared, |s| s.auto_reconnect);
                if !auto {
                    with_status(&shared, |s| {
                        s.phase = HubPhase::Error;
                        s.last_error = Some(err);
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
                        "disconnected ({err}); auto-reconnect in {delay_secs}s (#{attempt})"
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
    BRIDGE_RUNNING.store(false, Ordering::SeqCst);
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

static PENDING_PUSH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

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

    while session.state() != ConnectionState::Connected {
        if STOP_BRIDGE.load(Ordering::SeqCst) {
            let _ = session.handle(SessionEvent::Disconnect);
            return Ok(());
        }
        conn.set_read_timeout(Some(Duration::from_millis(200)))
            .map_err(|e| e.to_string())?;
        conn.set_nonblocking(false).map_err(|e| e.to_string())?;
        match conn.recv() {
            Ok(msg) => {
                session
                    .handle(SessionEvent::Message(msg))
                    .map_err(|e| e.to_string())?;
                conn.send_all(session.take_outbox().iter())
                    .map_err(|e| e.to_string())?;
                last_peer_rx = Instant::now();
            }
            Err(m590_net::TcpError::Io(err))
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(m590_net::TcpError::Disconnected) => return Err("peer disconnected".into()),
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

        loop {
            match conn.try_recv() {
                Ok(Some(msg)) => {
                    session
                        .handle(SessionEvent::Message(msg))
                        .map_err(|e| e.to_string())?;
                    last_peer_rx = Instant::now();
                    conn.send_all(session.take_outbox().iter())
                        .map_err(|e| e.to_string())?;
                    if let Some(InboundClipboardResult::Applied { content_id, text }) =
                        session.take_inbound_clipboard()
                    {
                        with_status(&shared, |s| {
                            s.last_sync_text = Some(text.clone());
                            s.last_sync_content_id = Some(content_id);
                        });
                        if let Some(clip) = clipboard.as_mut() {
                            let _ = clip.write_text(&text);
                        }
                    }
                }
                Ok(None) => break,
                Err(m590_net::TcpError::Disconnected) => return Err("peer disconnected".into()),
                Err(err) => return Err(err.to_string()),
            }
        }

        if let Some(clip) = clipboard.as_mut() {
            if let Ok(Some(text)) = clip.poll_text_change() {
                let auto = with_status(&shared, |s| s.auto_sync);
                if auto {
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

        thread::sleep(Duration::from_millis(50));
    }
}
