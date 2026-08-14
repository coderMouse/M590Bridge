//! Local preference file for hub/UI (device id, last pair params, toggles).

use std::fs;
use std::path::{Path, PathBuf};

use m590_net::DEFAULT_PORT;

/// Environment override for tests and custom installs.
pub const CONFIG_ENV: &str = "M590_CONFIG";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub device_id: String,
    pub last_role: Option<String>,
    pub pairing_code: Option<String>,
    pub listen_port: u16,
    pub connect_addr: Option<String>,
    pub auto_sync: bool,
    pub auto_reconnect: bool,
    /// Directory for received file-channel payloads (created on demand).
    pub file_save_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_id: default_device_id(),
            last_role: None,
            pairing_code: None,
            listen_port: DEFAULT_PORT,
            connect_addr: None,
            auto_sync: true,
            auto_reconnect: true,
            file_save_dir: default_file_save_dir().display().to_string(),
        }
    }
}

/// Stable-ish id: hostname + short random suffix (not PID — survives restarts without colliding as often across machines).
fn default_device_id() -> String {
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "m590".into());
    let mut label = String::with_capacity(host.len());
    let mut prev_hyphen = false;
    for c in host.chars() {
        if label.len() >= 24 {
            break;
        }
        if c.is_ascii_alphanumeric() {
            label.push(c.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !label.is_empty() && !prev_hyphen {
            // Collapse any run of non-alnum into a single hyphen.
            label.push('-');
            prev_hyphen = true;
        }
    }
    while label.ends_with('-') {
        label.pop();
    }
    if label.is_empty() {
        label.push_str("m590");
    }
    // 4 hex from mixed entropy so two fresh installs on same host rarely share id.
    let mix = std::process::id() as u64
        ^ std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
    format!("{label}-{:04x}", (mix & 0xffff) as u16)
}

/// Default inbox for received files (platform data dir / m590bridge/inbox).
pub fn default_file_save_dir() -> PathBuf {
    if let Some(mut base) = platform_data_dir() {
        base.push("m590bridge");
        base.push("inbox");
        return base;
    }
    PathBuf::from("m590bridge-inbox")
}

fn platform_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
    }
}

impl AppConfig {
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
        fn opt(v: &Option<String>) -> String {
            match v {
                Some(s) => format!("\"{}\"", esc(s)),
                None => "null".into(),
            }
        }
        format!(
            "{{\
\"device_id\":\"{device_id}\",\
\"last_role\":{last_role},\
\"pairing_code\":{pairing_code},\
\"listen_port\":{listen_port},\
\"connect_addr\":{connect_addr},\
\"auto_sync\":{auto_sync},\
\"auto_reconnect\":{auto_reconnect}\
}}",
            device_id = esc(&self.device_id),
            last_role = opt(&self.last_role),
            pairing_code = opt(&self.pairing_code),
            listen_port = self.listen_port,
            connect_addr = opt(&self.connect_addr),
            auto_sync = self.auto_sync,
            auto_reconnect = self.auto_reconnect,
        )
    }

    pub fn apply_json_patch(&mut self, body: &str) {
        if let Some(v) = json_str(body, "device_id") {
            if !v.is_empty() {
                self.device_id = v;
            }
        }
        if body.contains("\"last_role\"") {
            self.last_role = json_str(body, "last_role").filter(|s| !s.is_empty());
        }
        if body.contains("\"pairing_code\"") {
            self.pairing_code = json_str(body, "pairing_code").filter(|s| !s.is_empty());
        }
        if let Some(p) = json_str(body, "listen_port").and_then(|s| s.parse().ok()) {
            self.listen_port = p;
        }
        if body.contains("\"connect_addr\"") {
            self.connect_addr = json_str(body, "connect_addr").filter(|s| !s.is_empty());
        }
        if let Some(v) = json_bool(body, "auto_sync") {
            self.auto_sync = v;
        }
        if let Some(v) = json_bool(body, "auto_reconnect") {
            self.auto_reconnect = v;
        }
        if let Some(v) = json_str(body, "file_save_dir") {
            if !v.is_empty() {
                self.file_save_dir = v;
            }
        }
    }
}

pub fn default_config_path() -> PathBuf {
    if let Ok(p) = std::env::var(CONFIG_ENV) {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Some(path) = platform_config_path() {
        return path;
    }
    PathBuf::from("m590bridge.cfg")
}

fn platform_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(base).join("M590Bridge").join("config.cfg"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(xdg).join("m590bridge").join("config.cfg"));
        }
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("m590bridge")
                .join("config.cfg"),
        )
    }
}

pub fn load_config() -> AppConfig {
    load_config_from(&default_config_path())
}

pub fn load_config_from(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(raw) => parse_config(&raw),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(cfg: &AppConfig) -> Result<(), String> {
    save_config_to(&default_config_path(), cfg)
}

pub fn save_config_to(path: &Path, cfg: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
        }
    }
    fs::write(path, format_config(cfg)).map_err(|e| format!("write config: {e}"))
}

pub fn format_config(cfg: &AppConfig) -> String {
    let mut lines = vec![
        "# m590bridge local config v1".to_string(),
        format!("device_id={}", cfg.device_id),
        format!("listen_port={}", cfg.listen_port),
        format!("auto_sync={}", cfg.auto_sync),
        format!("auto_reconnect={}", cfg.auto_reconnect),
        format!("file_save_dir={}", cfg.file_save_dir),
    ];
    if let Some(role) = &cfg.last_role {
        lines.push(format!("last_role={role}"));
    }
    if let Some(code) = &cfg.pairing_code {
        lines.push(format!("pairing_code={code}"));
    }
    if let Some(addr) = &cfg.connect_addr {
        lines.push(format!("connect_addr={addr}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub fn parse_config(raw: &str) -> AppConfig {
    let mut cfg = AppConfig::default();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim();
        match k {
            "device_id" if !v.is_empty() => cfg.device_id = v.to_string(),
            "last_role" if !v.is_empty() => cfg.last_role = Some(v.to_string()),
            "pairing_code" if !v.is_empty() => cfg.pairing_code = Some(v.to_string()),
            "listen_port" => {
                if let Ok(p) = v.parse() {
                    cfg.listen_port = p;
                }
            }
            "connect_addr" if !v.is_empty() => cfg.connect_addr = Some(v.to_string()),
            "auto_sync" => cfg.auto_sync = parse_bool(v, cfg.auto_sync),
            "auto_reconnect" => cfg.auto_reconnect = parse_bool(v, cfg.auto_reconnect),
            "file_save_dir" if !v.is_empty() => cfg.file_save_dir = v.to_string(),
            _ => {}
        }
    }
    cfg
}

fn parse_bool(v: &str, default: bool) -> bool {
    match v.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn json_str(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let idx = body.find(&pat)?;
    let after = body[idx + pat.len()..]
        .trim_start()
        .trim_start_matches(':')
        .trim_start();
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
        return Some(out);
    }
    let token: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == ':' || *c == '-')
        .collect();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn json_bool(body: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{key}\"");
    let idx = body.find(&pat)?;
    let after = body[idx + pat.len()..]
        .trim_start()
        .trim_start_matches(':')
        .trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn roundtrip_file() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("m590-cfg-{nanos}.cfg"));
        let mut cfg = AppConfig::default();
        cfg.device_id = "dev-stable".into();
        cfg.last_role = Some("joiner".into());
        cfg.pairing_code = Some("654321".into());
        cfg.listen_port = 5902;
        cfg.connect_addr = Some("192.168.1.10:5901".into());
        cfg.auto_sync = false;
        cfg.auto_reconnect = true;
        cfg.file_save_dir = "/tmp/m590-inbox-test".into();
        save_config_to(&path, &cfg).expect("save");
        let loaded = load_config_from(&path);
        let _ = fs::remove_file(&path);
        assert_eq!(loaded.device_id, "dev-stable");
        assert_eq!(loaded.last_role.as_deref(), Some("joiner"));
        assert_eq!(loaded.pairing_code.as_deref(), Some("654321"));
        assert_eq!(loaded.listen_port, 5902);
        assert_eq!(loaded.connect_addr.as_deref(), Some("192.168.1.10:5901"));
        assert!(!loaded.auto_sync);
        assert!(loaded.auto_reconnect);
        assert_eq!(loaded.file_save_dir, "/tmp/m590-inbox-test");
    }

    #[test]
    fn apply_json_patch_partial() {
        let mut cfg = AppConfig::default();
        cfg.apply_json_patch(r#"{"auto_reconnect":false,"listen_port":5911,"connect_addr":"10.0.0.2:5901","file_save_dir":"/tmp/inbox2"}"#);
        assert!(!cfg.auto_reconnect);
        assert_eq!(cfg.listen_port, 5911);
        assert_eq!(cfg.connect_addr.as_deref(), Some("10.0.0.2:5901"));
        assert_eq!(cfg.file_save_dir, "/tmp/inbox2");
    }
}
