#[cfg(target_os = "windows")]
use std::io::{self, Read, Seek, SeekFrom};

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("windows_virtual_file example requires Windows");
}

#[cfg(target_os = "windows")]
fn main() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::thread;
    use std::time::Duration;

    use m590_clipboard::{publish_virtual_file, VirtualFile};

    let mut args = std::env::args().skip(1);
    let size = args
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(64 * 1024 * 1024);
    let delay_ms = args
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(2);

    let file = VirtualFile::new("M590Bridge-virtual.bin", size, move || {
        println!("content_opened size={size} delay_ms={delay_ms}");
        Ok(PatternReader::new(size, Duration::from_millis(delay_ms)))
    })
    .expect("virtual file descriptor should be valid");
    let clipboard = publish_virtual_file(file).expect("publish virtual file clipboard");
    println!("virtual_file_ready size={size}; press Ctrl+V in Explorer, then Enter to exit");

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

#[cfg(target_os = "windows")]
struct PatternReader {
    size: u64,
    position: u64,
    delay: std::time::Duration,
}

#[cfg(target_os = "windows")]
impl PatternReader {
    fn new(size: u64, delay: std::time::Duration) -> Self {
        Self {
            size,
            position: 0,
            delay,
        }
    }
}

#[cfg(target_os = "windows")]
impl Read for PatternReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.size {
            return Ok(0);
        }
        let bytes = buffer
            .len()
            .min((self.size - self.position) as usize)
            .min(64 * 1024);
        for (index, byte) in buffer[..bytes].iter_mut().enumerate() {
            *byte = ((self.position + index as u64) % 251) as u8;
        }
        self.position += bytes as u64;
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(bytes)
    }
}

#[cfg(target_os = "windows")]
impl Seek for PatternReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(offset) => self.position as i128 + offset as i128,
            SeekFrom::End(offset) => self.size as i128 + offset as i128,
        };
        if !(0..=self.size as i128).contains(&target) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek outside virtual file",
            ));
        }
        self.position = target as u64;
        Ok(self.position)
    }
}
