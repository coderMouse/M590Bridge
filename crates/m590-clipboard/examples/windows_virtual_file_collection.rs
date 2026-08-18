#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("windows_virtual_file_collection example requires Windows");
}

#[cfg(target_os = "windows")]
fn main() {
    use std::io::{self, Cursor};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::thread;
    use std::time::Duration;

    use m590_clipboard::{
        publish_virtual_file_collection, VirtualFile, VirtualFileCollection,
        VirtualFileCollectionEntry,
    };

    fn file(path: &str, contents: &'static [u8]) -> VirtualFileCollectionEntry {
        let file_name = path.rsplit('/').next().expect("test path has a base name");
        let path_for_log = path.to_string();
        let file = VirtualFile::new(file_name, contents.len() as u64, move || {
            println!("content_opened path={path_for_log}");
            Ok(Cursor::new(contents))
        })
        .expect("test file descriptor should be valid");
        VirtualFileCollectionEntry::file(path, file).expect("test collection path should be valid")
    }

    let collection = VirtualFileCollection::new(vec![
        file("top-a.txt", b"top a\n"),
        file("top-b.txt", b"top b\n"),
        VirtualFileCollectionEntry::directory("nested").expect("test directory should be valid"),
        file("nested/child.txt", b"nested child\n"),
        VirtualFileCollectionEntry::directory("nested/empty")
            .expect("test directory should be valid"),
        VirtualFileCollectionEntry::directory("empty-root")
            .expect("test directory should be valid"),
    ])
    .expect("test collection should be valid");
    let clipboard =
        publish_virtual_file_collection(collection).expect("publish virtual file collection");
    println!(
        "virtual_collection_ready expected=top-a.txt,top-b.txt,nested\\child.txt,nested\\empty,empty-root"
    );
    println!("Press Ctrl+V in Explorer, verify all entries, then press Enter here to exit.");

    let done = Arc::new(AtomicBool::new(false));
    let done_reader = Arc::clone(&done);
    thread::spawn(move || {
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
        done_reader.store(true, Ordering::Release);
    });

    while !done.load(Ordering::Acquire) {
        clipboard
            .pump_messages()
            .expect("pump OLE clipboard messages on owner thread");
        thread::sleep(Duration::from_millis(5));
    }
}
