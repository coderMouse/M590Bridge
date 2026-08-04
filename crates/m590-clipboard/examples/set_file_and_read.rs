use m590_clipboard::{image_from_paths, ClipboardService, PlatformClipboard};
use std::path::PathBuf;

fn main() {
    let path = PathBuf::from("/home/huang/图片/截图/截图 2026-07-29 17-19-37.png");
    assert!(path.is_file(), "screenshot missing: {}", path.display());

    // Set via arboard (dev-dep of this package on linux through target dep - available to examples?)
    let mut raw = arboard::Clipboard::new().expect("arboard open");
    raw.set()
        .file_list(&[path.as_path()])
        .expect("set file_list");

    let mut clip = PlatformClipboard::open().expect("platform open");
    let paths = clip.read_file_list().expect("read_file_list");
    println!("files={paths:?}");
    let img = image_from_paths(&paths)
        .expect("decode result")
        .expect("should decode image file");
    println!(
        "decoded={}x{} bytes={}",
        img.width,
        img.height,
        img.rgba.len()
    );
}
