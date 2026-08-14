use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "linux")]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::atomic::AtomicU64,
};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent, Wry,
};

#[cfg(target_os = "windows")]
use winreg::{
    enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
    RegKey,
};

const HUB_API: &str = "127.0.0.1:5910";

#[cfg(target_os = "linux")]
const AUTOSTART_DESKTOP_FILE: &str = "M590Bridge.desktop";

#[cfg(target_os = "linux")]
static AUTOSTART_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "windows")]
const WINDOWS_AUTOSTART_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(target_os = "windows")]
const WINDOWS_AUTOSTART_VALUE: &str = "M590Bridge";

struct HubAuthToken(String);

#[derive(Default)]
struct HubRuntimeState {
    ready: AtomicBool,
    error: Mutex<Option<String>>,
}

impl HubRuntimeState {
    fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.error.lock() {
            *slot = None;
        }
    }

    fn mark_error(&self, err: String) {
        self.ready.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.error.lock() {
            *slot = Some(err);
        }
    }

    fn snapshot(&self) -> HubRuntimeInfo {
        HubRuntimeInfo {
            ready: self.ready.load(Ordering::SeqCst),
            error: self.error.lock().ok().and_then(|guard| guard.clone()),
            api: format!("http://{HUB_API}"),
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct HubRuntimeInfo {
    ready: bool,
    error: Option<String>,
    api: String,
}

struct TrayState {
    /// Keep tray + menu items alive for the process lifetime (Linux AppIndicator).
    _tray: TrayIcon<Wry>,
    _show: MenuItem<Wry>,
    _quit: MenuItem<Wry>,
    _menu: Menu<Wry>,
}

fn desktop_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        let zh = home.join("桌面");
        if zh.is_dir() {
            return Some(zh);
        }
        let en = home.join("Desktop");
        if en.is_dir() {
            return Some(en);
        }
        return Some(home);
    }
    None
}

#[cfg(target_os = "linux")]
fn xdg_config_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Ok(path);
        }
    }

    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable; cannot resolve XDG config directory".to_string())?;
    if !home.is_absolute() {
        return Err("HOME must be an absolute path".into());
    }
    Ok(home.join(".config"))
}

#[cfg(target_os = "linux")]
fn autostart_path_from_config_home(config_home: &Path) -> PathBuf {
    config_home.join("autostart").join(AUTOSTART_DESKTOP_FILE)
}

#[cfg(target_os = "linux")]
fn desktop_exec_value(executable: &Path) -> Result<String, String> {
    if !executable.is_absolute() {
        return Err("autostart executable path must be absolute".into());
    }
    let raw = executable
        .to_str()
        .ok_or_else(|| "autostart executable path must be valid UTF-8".to_string())?;
    if raw.chars().any(char::is_control) {
        return Err("autostart executable path contains control characters".into());
    }

    let mut escaped = String::with_capacity(raw.len() + 2);
    escaped.push('"');
    for ch in raw.chars() {
        match ch {
            '\\' | '"' | '`' | '$' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            '%' => escaped.push_str("%%"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    Ok(escaped)
}

#[cfg(target_os = "linux")]
fn autostart_entry(executable: &Path) -> Result<String, String> {
    let exec = desktop_exec_value(executable)?;
    Ok(format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=M590Bridge\nComment=Start M590Bridge after login\nExec={exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    ))
}

#[cfg(target_os = "linux")]
fn autostart_enabled_at(config_home: &Path) -> bool {
    autostart_path_from_config_home(config_home).is_file()
}

fn ensure_autostart_build_supported(is_dev: bool) -> Result<(), String> {
    if is_dev {
        return Err(
            "开发版桌面壳依赖 Vite，不能用于登录自启；请运行 npm run desktop:standalone 或安装正式包后再开启"
                .into(),
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_autostart_at(config_home: &Path, executable: &Path, enabled: bool) -> Result<bool, String> {
    let path = autostart_path_from_config_home(config_home);
    if !enabled {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("remove {}: {err}", path.display())),
        }
        return Ok(false);
    }

    let parent = path
        .parent()
        .ok_or_else(|| "autostart path has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    let contents = autostart_entry(executable)?;
    let sequence = AUTOSTART_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = parent.join(format!(
        ".{AUTOSTART_DESKTOP_FILE}.tmp-{}-{sequence}",
        std::process::id()
    ));
    let write_result = (|| -> Result<(), String> {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|err| format!("create {}: {err}", temp_path.display()))?;
        temp.write_all(contents.as_bytes())
            .map_err(|err| format!("write {}: {err}", temp_path.display()))?;
        temp.sync_all()
            .map_err(|err| format!("sync {}: {err}", temp_path.display()))?;
        fs::rename(&temp_path, &path)
            .map_err(|err| format!("replace {}: {err}", path.display()))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result?;
    Ok(autostart_enabled_at(config_home))
}

#[cfg(any(target_os = "windows", test))]
fn windows_autostart_command_value(executable: &str) -> Result<String, String> {
    if executable.is_empty() {
        return Err("autostart executable path must not be empty".into());
    }
    if executable.chars().any(char::is_control) || executable.contains('"') {
        return Err("autostart executable path contains invalid characters".into());
    }
    Ok(format!("\"{executable}\""))
}

#[cfg(target_os = "windows")]
fn windows_autostart_command(executable: &Path) -> Result<String, String> {
    if !executable.is_absolute() {
        return Err("autostart executable path must be absolute".into());
    }
    let executable = executable
        .to_str()
        .ok_or_else(|| "autostart executable path must be valid UTF-8".to_string())?;
    windows_autostart_command_value(executable)
}

#[cfg(target_os = "windows")]
fn windows_autostart_enabled_for(executable: &Path) -> Result<bool, String> {
    let expected = windows_autostart_command(executable)?;
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match current_user.open_subkey(WINDOWS_AUTOSTART_KEY) {
        Ok(key) => key,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(format!("open Windows autostart registry key: {err}")),
    };
    match run_key.get_value::<String, _>(WINDOWS_AUTOSTART_VALUE) {
        Ok(value) => Ok(value == expected),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!("read Windows autostart registry value: {err}")),
    }
}

#[cfg(target_os = "windows")]
fn set_windows_autostart(executable: &Path, enabled: bool) -> Result<bool, String> {
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let command = windows_autostart_command(executable)?;
        let (run_key, _) = current_user
            .create_subkey(WINDOWS_AUTOSTART_KEY)
            .map_err(|err| format!("create Windows autostart registry key: {err}"))?;
        run_key
            .set_value(WINDOWS_AUTOSTART_VALUE, &command)
            .map_err(|err| format!("write Windows autostart registry value: {err}"))?;
        return windows_autostart_enabled_for(executable);
    }

    let run_key = match current_user.open_subkey_with_flags(WINDOWS_AUTOSTART_KEY, KEY_SET_VALUE) {
        Ok(key) => key,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(format!("open Windows autostart registry key: {err}")),
    };
    match run_key.delete_value(WINDOWS_AUTOSTART_VALUE) {
        Ok(()) => Ok(false),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!("delete Windows autostart registry value: {err}")),
    }
}

#[cfg(target_os = "linux")]
fn refresh_hidden_wayland_titlebar(window: &tauri::WebviewWindow) {
    if window.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(gtk_win) = window.gtk_window() else {
        return;
    };
    use gtk::prelude::*;

    if gtk_win.display().type_().name() != "GdkWaylandDisplay" {
        return;
    }

    // Recreate Tao's draggable CSD while the window is unmapped so stale
    // EventBox pointer state cannot survive into the next tray restore.
    let header = gtk::HeaderBar::builder()
        .show_close_button(true)
        .decoration_layout("menu:minimize,maximize,close")
        .title("M590Bridge")
        .build();
    let event_box = gtk::EventBox::new();
    event_box.set_above_child(true);
    event_box.set_visible(true);
    event_box.set_can_focus(false);
    event_box.add(&header);
    gtk_win.set_titlebar(Some(&event_box));
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "linux")]
        refresh_hidden_wayland_titlebar(&window);
        // Reverse close-to-tray state before asking the compositor to present the window.
        let _ = window.set_skip_taskbar(false);
        let _ = window.show();
        let _ = window.unminimize();
        #[cfg(target_os = "linux")]
        {
            if let Ok(gtk_win) = window.gtk_window() {
                use gtk::prelude::GtkWindowExt;
                gtk_win.present();
            }
        }
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        let _ = window.set_always_on_top(false);
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(tauri::UserAttentionType::Informational));
    }
}

fn validate_hub_api_path(path: &str) -> Result<(), String> {
    if !path.starts_with("/api/") {
        return Err("hub path must start with /api/".into());
    }
    if path.contains('\n') || path.contains('\r') || path.contains(' ') {
        return Err("hub path contains invalid characters".into());
    }
    if path.contains("://") || path.contains("..") {
        return Err("hub path is not allowed".into());
    }
    Ok(())
}

fn hub_http_exchange(
    method: &str,
    path: &str,
    body: &str,
    auth_token: &str,
) -> Result<(u16, String), String> {
    validate_hub_api_path(path)?;
    let method = method.trim().to_ascii_uppercase();
    if !matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
    ) {
        return Err(format!("unsupported hub method: {method}"));
    }
    let mut stream =
        std::net::TcpStream::connect(HUB_API).map_err(|e| format!("hub connect failed: {e}"))?;
    // Large send_file_bytes payloads need more than the default short timeout.
    let timeout_secs = if body.len() > 64 * 1024 { 60 } else { 5 };
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(timeout_secs)))
        .map_err(|e| format!("hub read timeout: {e}"))?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(timeout_secs)))
        .map_err(|e| format!("hub write timeout: {e}"))?;
    use std::io::{Read, Write};
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {HUB_API}\r\nContent-Type: application/json\r\nX-M590-Token: {auth_token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("hub write failed: {e}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("hub response failed: {e}"))?;
    let response = String::from_utf8_lossy(&response).into_owned();
    let (header, resp_body) = response
        .split_once("\r\n\r\n")
        .unwrap_or((response.as_str(), ""));
    let status = header
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("invalid hub response status: {header}"))?;
    Ok((status, resp_body.to_string()))
}

fn post_hub_send_file(path: &Path, auth_token: &str) -> Result<(), String> {
    let path_s = path.to_string_lossy();
    let body = serde_json::json!({ "path": path_s.as_ref() }).to_string();
    let (status, detail) = hub_http_exchange("POST", "/api/send_file", &body, auth_token)?;
    if status == 200 {
        Ok(())
    } else {
        Err(format!("hub rejected file: {detail}"))
    }
}

#[derive(serde::Deserialize)]
struct HubApiRequestArgs {
    method: String,
    path: String,
    body: Option<String>,
}

#[derive(serde::Serialize)]
struct HubApiResponse {
    status: u16,
    body: String,
}

/// Native file dialog (starts at Desktop/桌面). Returns absolute path.
#[tauri::command]
fn pick_send_file() -> Result<Option<String>, String> {
    let mut dialog = rfd::FileDialog::new().set_title("选择要发送的文件");
    if let Some(dir) = desktop_dir() {
        dialog = dialog.set_directory(dir);
    }
    Ok(dialog.pick_file().map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
fn hub_auth_token(token: tauri::State<'_, HubAuthToken>) -> String {
    token.0.clone()
}

#[tauri::command]
fn hub_runtime_info(state: tauri::State<'_, Arc<HubRuntimeState>>) -> HubRuntimeInfo {
    state.snapshot()
}

/// Desktop WebView proxy for the loopback Hub API.
/// Avoids mixed-content/CORS failures when the shell is served from https://tauri.localhost.
#[tauri::command]
fn hub_api_request(
    args: HubApiRequestArgs,
    token: tauri::State<'_, HubAuthToken>,
) -> Result<HubApiResponse, String> {
    let body = args.body.unwrap_or_default();
    let (status, body) = hub_http_exchange(&args.method, &args.path, &body, &token.0)?;
    Ok(HubApiResponse { status, body })
}

#[tauri::command]
fn autostart_enabled() -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        Ok(autostart_enabled_at(&xdg_config_home()?))
    }
    #[cfg(target_os = "windows")]
    {
        let executable =
            std::env::current_exe().map_err(|err| format!("resolve current executable: {err}"))?;
        windows_autostart_enabled_for(&executable)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(false)
    }
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        if enabled {
            ensure_autostart_build_supported(tauri::is_dev())?;
        }
        let executable =
            std::env::current_exe().map_err(|err| format!("resolve current executable: {err}"))?;
        set_autostart_at(&xdg_config_home()?, &executable, enabled)
    }
    #[cfg(target_os = "windows")]
    {
        if enabled {
            ensure_autostart_build_supported(tauri::is_dev())?;
        }
        let executable =
            std::env::current_exe().map_err(|err| format!("resolve current executable: {err}"))?;
        set_windows_autostart(&executable, enabled)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = enabled;
        Ok(false)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hub_token = m590_daemon::hub::generate_hub_token().expect("generate hub auth token");
    let hub_token_for_setup = hub_token.clone();
    let hub_runtime = Arc::new(HubRuntimeState::default());
    let hub_runtime_for_setup = Arc::clone(&hub_runtime);
    tauri::Builder::default()
        .manage(HubAuthToken(hub_token))
        .manage(hub_runtime)
        .invoke_handler(tauri::generate_handler![
            pick_send_file,
            hub_auth_token,
            hub_runtime_info,
            hub_api_request,
            autostart_enabled,
            set_autostart
        ])
        .setup(move |app| {
            let hub_token = hub_token_for_setup.clone();
            let hub_runtime = Arc::clone(&hub_runtime_for_setup);
            std::thread::Builder::new()
                .name("m590-hub".into())
                .spawn(move || {
                    let runtime = Arc::clone(&hub_runtime);
                    let result = m590_daemon::hub::run_hub_with_token_on_ready(
                        HUB_API,
                        hub_token,
                        Some(Box::new(move || runtime.mark_ready())),
                    );
                    if let Err(err) = result {
                        eprintln!("hub_error={err}");
                        hub_runtime.mark_error(err);
                    }
                })
                .expect("spawn hub thread");

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Explicit labels; keep owned items so Linux AppIndicator menu text stays valid.
            let show_i = MenuItem::with_id(app, "show", "打开主面板", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let icon = app
                .default_window_icon()
                .cloned()
                .expect("default window icon");
            if let Some(window) = app.get_webview_window("main") {
                window.set_icon(icon.clone())?;
            }

            let tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                // Linux AppIndicator always uses the menu; keep handler for Win/macOS.
                .show_menu_on_left_click(true)
                .tooltip("M590Bridge")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // On platforms that don't open the menu on left click, show window.
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            app.manage(TrayState {
                _tray: tray,
                _show: show_i,
                _quit: quit_i,
                _menu: menu,
            });

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // A minimized GTK window can remain visible in Ubuntu Dock even with
                // skip_taskbar set. Hide it fully and restore via show_main_window.
                api.prevent_close();
                let _ = window.set_skip_taskbar(true);
                #[cfg(target_os = "linux")]
                let _ = window.hide();
                #[cfg(not(target_os = "linux"))]
                let _ = window.minimize();
            }
            WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                if let Some(path) = paths.iter().find(|p| p.is_file()) {
                    let token = window.state::<HubAuthToken>();
                    if let Err(err) = post_hub_send_file(path, &token.0) {
                        eprintln!("drop_send_file_error={err}");
                    } else {
                        eprintln!("drop_send_file_ok={}", path.display());
                    }
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod hub_proxy_tests {
    use super::validate_hub_api_path;

    #[test]
    fn hub_api_path_validation_rejects_traversal_and_non_api() {
        assert!(validate_hub_api_path("/api/health").is_ok());
        assert!(validate_hub_api_path("/status").is_err());
        assert!(validate_hub_api_path("/api/../etc/passwd").is_err());
        assert!(validate_hub_api_path("/api/health http").is_err());
    }
}

#[cfg(test)]
mod windows_autostart_value_tests {
    use super::windows_autostart_command_value;

    #[test]
    fn windows_autostart_value_quotes_paths_with_spaces() {
        let value = windows_autostart_command_value(
            r"C:\Users\Example User\AppData\Local\M590Bridge\m590-ui.exe",
        )
        .unwrap();
        assert_eq!(
            value,
            r#""C:\Users\Example User\AppData\Local\M590Bridge\m590-ui.exe""#
        );
    }

    #[test]
    fn windows_autostart_value_rejects_quotes_and_controls() {
        assert!(windows_autostart_command_value("").is_err());
        assert!(windows_autostart_command_value("C:\\bad\"path.exe").is_err());
        assert!(windows_autostart_command_value("C:\\bad\npath.exe").is_err());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod autostart_tests {
    use super::*;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temp_config_home() -> PathBuf {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "m590-autostart-test-{}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn autostart_entry_roundtrips_in_xdg_directory() {
        let config_home = temp_config_home();
        let executable = Path::new("/opt/M590 Bridge/m590-ui");

        assert!(!autostart_enabled_at(&config_home));
        assert!(set_autostart_at(&config_home, executable, true).unwrap());

        let path = autostart_path_from_config_home(&config_home);
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Type=Application\n"));
        assert!(contents.contains("Exec=\"/opt/M590 Bridge/m590-ui\"\n"));
        assert!(contents.contains("X-GNOME-Autostart-enabled=true\n"));
        assert!(autostart_enabled_at(&config_home));

        assert!(!set_autostart_at(&config_home, executable, false).unwrap());
        assert!(!path.exists());
        assert!(!set_autostart_at(&config_home, executable, false).unwrap());
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn development_build_is_rejected_for_autostart() {
        let error = ensure_autostart_build_supported(true).unwrap_err();
        assert!(error.contains("desktop:standalone"));
        assert!(ensure_autostart_build_supported(false).is_ok());
    }

    #[test]
    fn tauri_build_mode_matches_autostart_policy() {
        let result = ensure_autostart_build_supported(tauri::is_dev());
        if cfg!(feature = "custom-protocol") {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn desktop_exec_escapes_field_codes_and_shell_characters() {
        let value = desktop_exec_value(Path::new("/opt/100%/$bridge`/m590-ui")).unwrap();
        assert_eq!(value, "\"/opt/100%%/\\$bridge\\`/m590-ui\"");
    }

    #[test]
    fn desktop_exec_rejects_relative_or_control_paths() {
        assert!(desktop_exec_value(Path::new("m590-ui")).is_err());
        assert!(desktop_exec_value(Path::new("/opt/m590-ui\nother")).is_err());
    }
}
