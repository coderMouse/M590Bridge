use std::path::{Path, PathBuf};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent, Wry,
};

const HUB_API: &str = "127.0.0.1:5910";

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hub_token = m590_daemon::hub::generate_hub_token().expect("generate hub auth token");
    let hub_token_for_setup = hub_token.clone();
    tauri::Builder::default()
        .manage(HubAuthToken(hub_token))
        .invoke_handler(tauri::generate_handler![pick_send_file, hub_auth_token])
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
                if let Some(path) = paths.into_iter().find(|p| p.is_file()) {
                    let token = window.state::<HubAuthToken>();
                    if let Err(err) = post_hub_send_file(&path, &token.0) {
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
