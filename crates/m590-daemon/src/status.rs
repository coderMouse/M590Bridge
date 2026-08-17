//! Shared runtime status for CLI bridge and UI hub API.

use std::sync::{Arc, Mutex};

use m590_core::ConnectionState;

use crate::config::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubPhase {
    Idle,
    WaitingPeer,
    Pairing,
    Connected,
    Error,
}

impl HubPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::WaitingPeer => "waiting_peer",
            Self::Pairing => "pairing",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HubStatus {
    pub phase: HubPhase,
    pub role: Option<String>,
    pub device_id: String,
    pub peer_device: Option<String>,
    pub pairing_code: Option<String>,
    pub endpoint: Option<String>,
    pub connection: Option<ConnectionState>,
    pub last_sync_text: Option<String>,
    pub last_sync_content_id: Option<String>,
    pub last_error: Option<String>,
    pub auto_sync: bool,
    pub auto_reconnect: bool,
    pub reconnect_attempt: u32,
    pub last_role: Option<String>,
    pub listen_port: u16,
    pub connect_addr: Option<String>,
    pub hub_api: Option<String>,
    pub file_save_dir: String,
    /// idle | offered | sending | receiving | done | failed
    pub file_transfer_phase: Option<String>,
    pub last_file_transfer_id: Option<String>,
    pub last_file_name: Option<String>,
    pub last_file_bytes: Option<u64>,
    pub last_file_saved_path: Option<String>,
    pub file_bytes_received: Option<u64>,
    pub file_bytes_total: Option<u64>,
    /// False on Linux Wayland without data-control: file-manager copy may be invisible.
    pub file_clipboard_watch_likely: bool,
}

impl Default for HubStatus {
    fn default() -> Self {
        let cfg = AppConfig::default();
        Self::from_config(&cfg)
    }
}

impl HubStatus {
    pub fn from_config(cfg: &AppConfig) -> Self {
        Self {
            phase: HubPhase::Idle,
            role: None,
            device_id: cfg.device_id.clone(),
            peer_device: None,
            pairing_code: cfg.pairing_code.clone(),
            endpoint: None,
            connection: None,
            last_sync_text: None,
            last_sync_content_id: None,
            last_error: None,
            auto_sync: cfg.auto_sync,
            auto_reconnect: cfg.auto_reconnect,
            reconnect_attempt: 0,
            last_role: cfg.last_role.clone(),
            listen_port: cfg.listen_port,
            connect_addr: cfg.connect_addr.clone(),
            hub_api: None,
            file_save_dir: cfg.file_save_dir.clone(),
            file_transfer_phase: None,
            last_file_transfer_id: None,
            last_file_name: None,
            last_file_bytes: None,
            last_file_saved_path: None,
            file_bytes_received: None,
            file_bytes_total: None,
            file_clipboard_watch_likely: m590_clipboard::file_clipboard_watch_likely(),
        }
    }

    pub fn snapshot_config(&self) -> AppConfig {
        AppConfig {
            device_id: self.device_id.clone(),
            last_role: self.last_role.clone().or_else(|| self.role.clone()),
            pairing_code: self.pairing_code.clone(),
            listen_port: self.listen_port,
            connect_addr: self.connect_addr.clone(),
            auto_sync: self.auto_sync,
            auto_reconnect: self.auto_reconnect,
            file_save_dir: self.file_save_dir.clone(),
        }
    }

    pub fn apply_config(&mut self, cfg: &AppConfig) {
        self.device_id = cfg.device_id.clone();
        self.last_role = cfg.last_role.clone();
        self.pairing_code = cfg.pairing_code.clone();
        self.listen_port = cfg.listen_port;
        self.connect_addr = cfg.connect_addr.clone();
        self.auto_sync = cfg.auto_sync;
        self.auto_reconnect = cfg.auto_reconnect;
        self.file_save_dir = cfg.file_save_dir.clone();
    }

    pub fn clear_file_transfer(&mut self) {
        self.file_transfer_phase = None;
        self.last_file_transfer_id = None;
        self.last_file_name = None;
        self.last_file_bytes = None;
        self.last_file_saved_path = None;
        self.file_bytes_received = None;
        self.file_bytes_total = None;
    }

    pub fn to_json(&self) -> String {
        fn esc(s: &str) -> String {
            let mut out = String::with_capacity(s.len() + 8);
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out
        }
        fn opt_str(v: &Option<String>) -> String {
            match v {
                Some(s) => format!("\"{}\"", esc(s)),
                None => "null".into(),
            }
        }
        fn opt_u64(v: Option<u64>) -> String {
            match v {
                Some(n) => n.to_string(),
                None => "null".into(),
            }
        }
        format!(
            "{{\
\"phase\":\"{phase}\",\
\"role\":{role},\
\"device_id\":\"{device_id}\",\
\"peer_device\":{peer},\
\"pairing_code\":{code},\
\"endpoint\":{endpoint},\
\"connection\":{conn},\
\"last_sync_text\":{last_text},\
\"last_sync_content_id\":{last_id},\
\"last_error\":{err},\
\"auto_sync\":{auto},\
\"auto_reconnect\":{auto_re},\
\"reconnect_attempt\":{attempt},\
\"last_role\":{last_role},\
\"listen_port\":{listen_port},\
\"connect_addr\":{connect_addr},\
\"hub_api\":{api},\
\"file_save_dir\":\"{file_save_dir}\",\
\"file_transfer_phase\":{file_phase},\
\"last_file_transfer_id\":{file_tid},\
\"last_file_name\":{file_name},\
\"last_file_bytes\":{file_bytes},\
\"last_file_saved_path\":{file_path},\
\"file_bytes_received\":{file_rx},\
\"file_bytes_total\":{file_total},\
\"file_clipboard_watch_likely\":{file_watch}\
}}",
            phase = self.phase.as_str(),
            role = opt_str(&self.role),
            device_id = esc(&self.device_id),
            peer = opt_str(&self.peer_device),
            code = opt_str(&self.pairing_code),
            endpoint = opt_str(&self.endpoint),
            conn = match self.connection {
                Some(c) => format!("\"{c:?}\""),
                None => "null".into(),
            },
            last_text = opt_str(&self.last_sync_text),
            last_id = opt_str(&self.last_sync_content_id),
            err = opt_str(&self.last_error),
            auto = self.auto_sync,
            auto_re = self.auto_reconnect,
            attempt = self.reconnect_attempt,
            last_role = opt_str(&self.last_role),
            listen_port = self.listen_port,
            connect_addr = opt_str(&self.connect_addr),
            api = opt_str(&self.hub_api),
            file_save_dir = esc(&self.file_save_dir),
            file_phase = opt_str(&self.file_transfer_phase),
            file_tid = opt_str(&self.last_file_transfer_id),
            file_name = opt_str(&self.last_file_name),
            file_bytes = opt_u64(self.last_file_bytes),
            file_path = opt_str(&self.last_file_saved_path),
            file_rx = opt_u64(self.file_bytes_received),
            file_total = opt_u64(self.file_bytes_total),
            file_watch = self.file_clipboard_watch_likely,
        )
    }
}

pub type SharedStatus = Arc<Mutex<HubStatus>>;

pub fn new_shared_status() -> SharedStatus {
    let cfg = crate::config::load_config();
    Arc::new(Mutex::new(HubStatus::from_config(&cfg)))
}

pub fn with_status<R>(shared: &SharedStatus, f: impl FnOnce(&mut HubStatus) -> R) -> R {
    let mut guard = shared.lock().expect("status lock");
    f(&mut guard)
}

pub fn persist_status_config(shared: &SharedStatus) {
    let cfg = with_status(shared, |s| s.snapshot_config());
    if let Err(err) = crate::config::save_config(&cfg) {
        eprintln!("config_save_error={err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_json_includes_file_fields() {
        let s = HubStatus {
            file_save_dir: "/tmp/inbox".into(),
            file_transfer_phase: Some("done".into()),
            last_file_name: Some("a.txt".into()),
            last_file_bytes: Some(3),
            last_file_saved_path: Some("/tmp/inbox/a.txt".into()),
            file_bytes_received: Some(3),
            file_bytes_total: Some(3),
            ..HubStatus::default()
        };
        let json = s.to_json();
        assert!(json.contains("file_save_dir"), "{json}");
        assert!(json.contains("/tmp/inbox"), "{json}");
        assert!(json.contains("file_transfer_phase"), "{json}");
        assert!(json.contains("last_file_name"), "{json}");
        assert!(json.contains("a.txt"), "{json}");
        assert!(json.contains("file_bytes_total"), "{json}");
        assert!(json.contains("file_clipboard_watch_likely"), "{json}");
    }
}
