//! LAN device discovery via mDNS / DNS-SD (task-029).
//!
//! Service type: `_m590bridge._tcp.local.`
//! TXT: `id=<device_id>`, `ver=<app_version>` — never pairing_code.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use m590_core::VERSION;

/// DNS-SD service type for M590Bridge peer discovery.
pub const SERVICE_TYPE: &str = "_m590bridge._tcp.local.";

#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub fullname: String,
    pub name: String,
    pub device_id: String,
    pub host: String,
    pub port: u16,
    /// Preferred `host:port` (IPv4 preferred; IPv6 bracketed).
    pub addr: String,
    pub last_seen_unix_ms: u64,
}

pub struct DiscoveryHandle {
    daemon: ServiceDaemon,
    peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    /// Shared so browse thread can filter self without holding Arc to Self (avoids cycle).
    advertised_fullname: Arc<Mutex<Option<String>>>,
    local_device_id: Arc<Mutex<String>>,
    advertising: Mutex<bool>,
}

impl DiscoveryHandle {
    /// Start mDNS daemon + browse for peers. Fails if multicast socket cannot be created.
    pub fn start(local_device_id: String) -> Result<Arc<Self>, String> {
        let daemon = ServiceDaemon::new().map_err(|e| format!("mdns daemon: {e}"))?;
        let peers = Arc::new(Mutex::new(HashMap::new()));
        let local_device_id = Arc::new(Mutex::new(local_device_id));
        let advertised_fullname = Arc::new(Mutex::new(None));

        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| format!("mdns browse: {e}"))?;

        let peers_thread = Arc::clone(&peers);
        let local_id_thread = Arc::clone(&local_device_id);
        let advertised_thread = Arc::clone(&advertised_fullname);
        thread::Builder::new()
            .name("m590-mdns-browse".into())
            .spawn(move || {
                while let Ok(event) = receiver.recv() {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let fullname = info.get_fullname().to_string();
                            let our_fullname = advertised_thread
                                .lock()
                                .ok()
                                .and_then(|g| g.clone());
                            if our_fullname.as_deref() == Some(fullname.as_str()) {
                                continue;
                            }
                            let device_id = info
                                .get_property_val_str("id")
                                .unwrap_or("")
                                .to_string();
                            let local_id = local_id_thread
                                .lock()
                                .map(|g| g.clone())
                                .unwrap_or_default();
                            if !device_id.is_empty() && device_id == local_id {
                                continue;
                            }
                            let Some(addr) = pick_connect_addr(info.as_ref()) else {
                                continue;
                            };
                            let name = instance_from_fullname(&fullname, SERVICE_TYPE);
                            let peer = DiscoveredPeer {
                                fullname: fullname.clone(),
                                name,
                                device_id,
                                host: info.get_hostname().to_string(),
                                port: info.get_port(),
                                addr,
                                last_seen_unix_ms: now_unix_ms(),
                            };
                            if let Ok(mut map) = peers_thread.lock() {
                                map.insert(fullname, peer);
                            }
                        }
                        ServiceEvent::ServiceRemoved(_ty, fullname) => {
                            if let Ok(mut map) = peers_thread.lock() {
                                map.remove(&fullname);
                            }
                        }
                        ServiceEvent::SearchStopped(_) => break,
                        _ => {}
                    }
                }
            })
            .map_err(|e| format!("mdns browse thread: {e}"))?;

        Ok(Arc::new(Self {
            daemon,
            peers,
            advertised_fullname,
            local_device_id,
            advertising: Mutex::new(false),
        }))
    }

    pub fn set_local_device_id(&self, id: &str) {
        if let Ok(mut g) = self.local_device_id.lock() {
            *g = id.to_string();
        }
    }

    /// Advertise this host as listening on `port`. Replaces any previous advertisement.
    pub fn advertise(&self, device_id: &str, port: u16) -> Result<(), String> {
        self.set_local_device_id(device_id);
        self.stop_advertise();

        let instance = sanitize_label(device_id);
        let host_name = format!("{instance}.local.");
        let props = [("id", device_id), ("ver", VERSION)];
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host_name, "", port, &props[..])
            .map_err(|e| format!("mdns ServiceInfo: {e}"))?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        self.daemon
            .register(info)
            .map_err(|e| format!("mdns register: {e}"))?;
        if let Ok(mut g) = self.advertised_fullname.lock() {
            *g = Some(fullname);
        }
        if let Ok(mut g) = self.advertising.lock() {
            *g = true;
        }
        println!("mdns_advertise=on type={SERVICE_TYPE} instance={instance} port={port}");
        Ok(())
    }

    pub fn stop_advertise(&self) {
        let fullname = self
            .advertised_fullname
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(name) = fullname {
            match self.daemon.unregister(&name) {
                Ok(rx) => {
                    // Best-effort wait so goodbye packets can go out.
                    let _ = rx.recv_timeout(Duration::from_millis(500));
                }
                Err(err) => eprintln!("mdns_unregister_error={err}"),
            }
            println!("mdns_advertise=off fullname={name}");
        }
        if let Ok(mut g) = self.advertising.lock() {
            *g = false;
        }
    }

    pub fn is_advertising(&self) -> bool {
        self.advertising.lock().map(|g| *g).unwrap_or(false)
    }

    pub fn list_peers(&self) -> Vec<DiscoveredPeer> {
        let mut peers: Vec<_> = self
            .peers
            .lock()
            .map(|g| g.values().cloned().collect())
            .unwrap_or_default();
        peers.sort_by(|a, b| a.name.cmp(&b.name).then(a.addr.cmp(&b.addr)));
        peers
    }

    pub fn to_json(&self) -> String {
        let peers = self.list_peers();
        let mut items = Vec::with_capacity(peers.len());
        for p in &peers {
            items.push(format!(
                "{{\
\"name\":\"{name}\",\
\"device_id\":\"{device_id}\",\
\"host\":\"{host}\",\
\"port\":{port},\
\"addr\":\"{addr}\",\
\"fullname\":\"{fullname}\",\
\"last_seen_unix_ms\":{seen}\
}}",
                name = esc(&p.name),
                device_id = esc(&p.device_id),
                host = esc(&p.host),
                port = p.port,
                addr = esc(&p.addr),
                fullname = esc(&p.fullname),
                seen = p.last_seen_unix_ms,
            ));
        }
        format!(
            "{{\
\"service_type\":\"{ty}\",\
\"advertising\":{adv},\
\"peers\":[{peers}]\
}}",
            ty = esc(SERVICE_TYPE),
            adv = self.is_advertising(),
            peers = items.join(","),
        )
    }
}

impl Drop for DiscoveryHandle {
    fn drop(&mut self) {
        self.stop_advertise();
        let _ = self.daemon.shutdown();
    }
}

fn pick_connect_addr(info: &mdns_sd::ResolvedService) -> Option<String> {
    let port = info.get_port();
    let v4s = info.get_addresses_v4();
    for ip in &v4s {
        if !ip.is_loopback() && !ip.is_unspecified() && !ip.is_link_local() {
            return Some(format!("{ip}:{port}"));
        }
    }
    for ip in &v4s {
        if !ip.is_loopback() && !ip.is_unspecified() {
            return Some(format!("{ip}:{port}"));
        }
    }
    for ip in &v4s {
        if !ip.is_unspecified() {
            return Some(format!("{ip}:{port}"));
        }
    }
    for scoped in info.get_addresses() {
        match scoped.to_ip_addr() {
            IpAddr::V6(v6) if !v6.is_loopback() && !v6.is_unspecified() => {
                return Some(format!("[{v6}]:{port}"));
            }
            _ => {}
        }
    }
    None
}

fn instance_from_fullname(fullname: &str, service_type: &str) -> String {
    let ty = service_type.trim_end_matches('.');
    let full = fullname.trim_end_matches('.');
    if let Some(prefix) = full.strip_suffix(ty) {
        prefix.trim_end_matches('.').to_string()
    } else if let Some((inst, _)) = full.split_once('.') {
        inst.to_string()
    } else {
        full.to_string()
    }
}

/// DNS-SD instance / host label: lowercase alnum + hyphen, max 48.
pub fn sanitize_label(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(48));
    for c in raw.chars() {
        if out.len() >= 48 {
            break;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '-' || c == '_' || c == '.' || c == ' ' {
            if out.ends_with('-') || out.is_empty() {
                continue;
            }
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "m590".into()
    } else {
        out
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_label_basic() {
        assert_eq!(sanitize_label("Desk-PC_01"), "desk-pc-01");
        assert_eq!(sanitize_label("!!!"), "m590");
        assert_eq!(sanitize_label("A B"), "a-b");
    }

    #[test]
    fn instance_from_fullname_strips_type() {
        let full = "desk-pc._m590bridge._tcp.local.";
        assert_eq!(instance_from_fullname(full, SERVICE_TYPE), "desk-pc");
    }

    #[test]
    fn discover_json_empty_shape() {
        // Constructing ServiceDaemon may fail in restricted sandboxes — skip then.
        let Ok(handle) = DiscoveryHandle::start("test-device".into()) else {
            return;
        };
        let json = handle.to_json();
        assert!(json.contains("\"peers\":["));
        assert!(json.contains("\"service_type\":"));
        assert!(json.contains("\"advertising\":false"));
        handle.stop_advertise();
    }
}
