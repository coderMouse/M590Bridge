use m590_clipboard::{image_from_paths, ClipboardService, PlatformClipboard};
fn main() {
    match PlatformClipboard::open() {
        Ok(mut c) => {
            println!("backend={:?}", c.backend());
            println!("text={:?}", c.read_text());
            println!("files={:?}", c.read_file_list());
            match c.read_image() {
                Ok(Some(i)) => println!("image={}x{}", i.width, i.height),
                Ok(None) => println!("image=None"),
                Err(e) => println!("image_err={e}"),
            }
            if let Ok(files) = c.read_file_list() {
                println!("from_paths={:?}", image_from_paths(&files).map(|o| o.map(|i| (i.width,i.height))));
            }
        }
        Err(e) => println!("open_err={e}"),
    }
}
