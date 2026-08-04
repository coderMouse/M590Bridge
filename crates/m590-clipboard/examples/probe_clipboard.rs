use m590_clipboard::{
    image_from_clipboard_text, image_from_paths, ClipboardService, PlatformClipboard,
};
use std::thread;
use std::time::Duration;

fn dump(label: &str, clip: &mut PlatformClipboard) {
    println!("--- {label} ---");
    match clip.read_text() {
        Ok(t) => println!("text={t:?}"),
        Err(e) => println!("text_err={e}"),
    }
    match clip.read_image() {
        Ok(Some(img)) => println!("image={}x{} bytes={}", img.width, img.height, img.rgba.len()),
        Ok(None) => println!("image=None"),
        Err(e) => println!("image_err={e}"),
    }
    match clip.read_file_list() {
        Ok(paths) => println!("files={paths:?}"),
        Err(e) => println!("files_err={e}"),
    }
    if let Ok(paths) = clip.read_file_list() {
        match image_from_paths(&paths) {
            Ok(Some(img)) => println!(
                "from_files={}x{} bytes={}",
                img.width,
                img.height,
                img.rgba.len()
            ),
            Ok(None) => println!("from_files=None"),
            Err(e) => println!("from_files_err={e}"),
        }
    }
    if let Ok(Some(t)) = clip.read_text() {
        match image_from_clipboard_text(&t) {
            Ok(Some(img)) => println!(
                "from_text_path={}x{} bytes={}",
                img.width,
                img.height,
                img.rgba.len()
            ),
            Ok(None) => println!("from_text_path=None"),
            Err(e) => println!("from_text_path_err={e}"),
        }
    }
}

fn main() {
    let mut clip = PlatformClipboard::open().expect("open clipboard");
    println!("backend={:?}", clip.backend());
    dump("initial", &mut clip);
    println!("Copy an image FILE in the file manager within 20s...");
    for i in 1..=40 {
        thread::sleep(Duration::from_millis(500));
        if let Ok(Some(paths)) = clip.poll_file_list_change() {
            println!("poll_files#{i} => {paths:?}");
            match image_from_paths(&paths) {
                Ok(Some(img)) => println!(
                    "  decoded {}x{} bytes={}",
                    img.width,
                    img.height,
                    img.rgba.len()
                ),
                Ok(None) => println!("  no image among files"),
                Err(e) => println!("  decode_err={e}"),
            }
        }
        if let Ok(Some(t)) = clip.poll_text_change() {
            println!("poll_text#{i} => {t:?}");
        }
        if let Ok(Some(img)) = clip.poll_image_change() {
            println!(
                "poll_image#{i} => {}x{} bytes={}",
                img.width,
                img.height,
                img.rgba.len()
            );
        }
    }
    dump("final", &mut clip);
}
