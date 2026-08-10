use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent, Wry,
};

const HUB_API: &str = "127.0.0.1:5910";

#[cfg(target_os = "linux")]
const AUTOSTART_DESKTOP_FILE: &str = "M590Bridge.desktop";

#[cfg(target_os = "linux")]
static AUTOSTART_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct HubAuthToken(String);

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

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        // Close-to-tray uses minimize+skip_taskbar; reverse that fully on restore.
        let _ = window.set_skip_taskbar(false);
        let _ = window.unminimize();
        let _ = window.show();
        // GNOME/Wayland often ignores set_focus alone after tray restore.
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

fn post_hub_send_file(path: &Path, auth_token: &str) -> Result<(), String> {
    let path_s = path.to_string_lossy();
    let body = serde_json::json!({ "path": path_s.as_ref() }).to_string();
    let mut stream =
        std::net::TcpStream::connect(HUB_API).map_err(|e| format!("hub connect failed: {e}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| format!("hub read timeout: {e}"))?;
    use std::io::{Read, Write};
    let req = format!(
        "POST /api/send_file HTTP/1.1\r\nHost: {HUB_API}\r\nContent-Type: application/json\r\nX-M590-Token: {auth_token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("hub write failed: {e}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("hub response failed: {e}"))?;
    if response.starts_with("HTTP/1.1 200 ") {
        Ok(())
    } else {
        let detail = response
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or("request failed");
        Err(format!("hub rejected file: {detail}"))
    }
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
fn autostart_enabled() -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        Ok(autostart_enabled_at(&xdg_config_home()?))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(false)
    }
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        let executable =
            std::env::current_exe().map_err(|err| format!("resolve current executable: {err}"))?;
        set_autostart_at(&xdg_config_home()?, &executable, enabled)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = enabled;
        Ok(false)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hub_token = m590_daemon::hub::generate_hub_token().expect("generate hub auth token");
    let hub_token_for_setup = hub_token.clone();
    tauri::Builder::default()
        .manage(HubAuthToken(hub_token))
        .invoke_handler(tauri::generate_handler![
            pick_send_file,
            hub_auth_token,
            autostart_enabled,
            set_autostart
        ])
        .setup(move |app| {
            let hub_token = hub_token_for_setup.clone();
            std::thread::Builder::new()
                .name("m590-hub".into())
                .spawn(move || {
                    if let Err(err) = m590_daemon::hub::run_hub_with_token(HUB_API, hub_token) {
                        eprintln!("hub_error={err}");
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
                // Close to tray: minimize + hide from taskbar.
                // Full `hide()` on GNOME Wayland often leaves title-bar buttons unclickable
                // until the webview is clicked first.
                api.prevent_close();
                let _ = window.set_skip_taskbar(true);
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
