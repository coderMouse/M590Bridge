#[cfg(target_os = "linux")]
use std::{env, fs, io, path::PathBuf};

#[cfg(target_os = "linux")]
use m590_clipboard::{file_clipboard_watch_likely, ClipboardService, PlatformClipboard};

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("set_file_and_read example requires Linux");
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), String> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "set_file_and_read".into());
    let raw_path = args.next().ok_or_else(|| usage(&program))?;
    if args.next().is_some() {
        return Err(usage(&program));
    }

    let path = fs::canonicalize(PathBuf::from(raw_path))
        .map_err(|error| format!("cannot resolve file path: {error}"))?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("path is not a regular file: {}", path.display()));
    }

    let data_control_available = file_clipboard_watch_likely();
    let publisher_backend = publisher_backend(data_control_available);
    let mut clipboard =
        PlatformClipboard::open().map_err(|error| format!("cannot open clipboard: {error}"))?;

    println!("session_backend={:?}", clipboard.backend());
    println!("data_control_available={data_control_available}");
    println!("publisher_backend={publisher_backend}");
    println!("published_mime=text/uri-list");

    clipboard
        .write_file_list(std::slice::from_ref(&path))
        .map_err(|error| format!("cannot publish file URI: {error}"))?;

    let readback = clipboard
        .read_file_list()
        .map_err(|error| format!("cannot read back file URI: {error}"))?;
    let readback_matches = readback.iter().any(|candidate| candidate == &path);
    println!("published_file={}", path.display());
    println!("readback_matches={readback_matches}");
    println!("nautilus_test_required=true");
    if !readback_matches {
        return Err(format!(
            "published URI was not present on readback: {readback:?}"
        ));
    }

    println!("Press Ctrl+V in a Nautilus target directory, then press Enter here to exit.");
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|error| format!("cannot wait for input: {error}"))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn usage(program: &std::ffi::OsStr) -> String {
    format!("usage: {} <local-file-path>", program.to_string_lossy())
}

#[cfg(target_os = "linux")]
fn publisher_backend(data_control_available: bool) -> &'static str {
    let wayland = env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
    let x11 = env::var_os("DISPLAY").is_some_and(|value| !value.is_empty());
    match (wayland, data_control_available, x11) {
        (true, true, _) => "wayland-data-control",
        (true, false, true) => "x11-fallback",
        (false, _, true) => "x11",
        _ => "unavailable",
    }
}
