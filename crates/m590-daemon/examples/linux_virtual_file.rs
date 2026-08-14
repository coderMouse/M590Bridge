#[cfg(target_os = "linux")]
use std::fs::{self, File};
#[cfg(target_os = "linux")]
use std::io::{self, Read, Seek, SeekFrom};
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux_virtual_file example requires Linux");
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
    use m590_daemon::linux_virtual_file::{LinuxVirtualFile, LinuxVirtualFileMount};

    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_else(|| "linux_virtual_file".into());
    let size = parse_u64(args.next(), 256 * 1024 * 1024, "size-bytes", &program)?;
    let delay_ms = parse_u64(args.next(), 2, "delay-ms", &program)?;
    let verify_path = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err(usage(&program));
    }

    let mount_point = std::env::temp_dir().join(format!(
        "m590bridge-fuse-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("cannot create mount id: {error}"))?
            .as_nanos()
    ));
    fs::create_dir(&mount_point).map_err(|error| {
        format!(
            "cannot create temporary FUSE mount point {}: {error}",
            mount_point.display()
        )
    })?;

    let file = LinuxVirtualFile::new("M590Bridge-virtual.bin", size, move || {
        println!("content_opened size={size} delay_ms={delay_ms}");
        Ok(PatternReader::new(size, Duration::from_millis(delay_ms)))
    })
    .map_err(|error| format!("cannot create virtual file: {error}"))?;

    let mount = match LinuxVirtualFileMount::mount(&mount_point, file) {
        Ok(mount) => mount,
        Err(error) => {
            let _ = fs::remove_dir(&mount_point);
            return Err(format!("cannot mount FUSE virtual file: {error}"));
        }
    };

    let clipboard_result = publish_and_wait(&mount, verify_path.as_deref());
    let unmount_result = mount
        .unmount()
        .map_err(|error| format!("cannot unmount FUSE virtual file: {error}"));
    let cleanup_result = fs::remove_dir(&mount_point)
        .map_err(|error| format!("cannot remove temporary mount point: {error}"));

    clipboard_result?;
    unmount_result?;
    cleanup_result?;
    if let Some(path) = verify_path {
        verify_pasted_file(&path, size)?;
        println!("pasted_file_verified=true path={}", path.display());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn publish_and_wait(
    mount: &m590_daemon::linux_virtual_file::LinuxVirtualFileMount,
    verify_path: Option<&Path>,
) -> Result<(), String> {
    use m590_clipboard::{ClipboardService, PlatformClipboard};

    let mut clipboard =
        PlatformClipboard::open().map_err(|error| format!("cannot open clipboard: {error}"))?;
    let file_path = mount.file_path();
    fs::metadata(file_path)
        .map_err(|error| format!("FUSE file metadata is unavailable: {error}"))?;
    clipboard
        .write_file_list(&[file_path.to_path_buf()])
        .map_err(|error| format!("cannot publish FUSE file URI: {error}"))?;

    println!(
        "virtual_file_ready size={} path={}",
        file_path
            .metadata()
            .map_err(|error| format!("cannot inspect FUSE file: {error}"))?
            .len(),
        file_path.display()
    );
    if let Some(path) = verify_path {
        println!("verify_after_paste={}", path.display());
    }
    println!("Press Ctrl+V in a Nautilus target directory, then press Enter here to exit.");
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|error| format!("cannot wait for input: {error}"))?;
    drop(clipboard);
    Ok(())
}

#[cfg(target_os = "linux")]
fn verify_pasted_file(path: &Path, expected_size: u64) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("cannot inspect pasted file: {error}"))?;
    if metadata.len() != expected_size {
        return Err(format!(
            "pasted file size mismatch: got {} expected {expected_size}",
            metadata.len()
        ));
    }

    let mut file = File::open(path).map_err(|error| format!("cannot open pasted file: {error}"))?;
    let mut offset = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read pasted file: {error}"))?;
        if count == 0 {
            break;
        }
        for (index, byte) in buffer[..count].iter().enumerate() {
            let expected = ((offset + index as u64) % 251) as u8;
            if *byte != expected {
                return Err(format!(
                    "pasted file content mismatch at offset {}: got {} expected {expected}",
                    offset + index as u64,
                    byte
                ));
            }
        }
        offset += count as u64;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn parse_u64(
    raw: Option<std::ffi::OsString>,
    default: u64,
    label: &str,
    program: &std::ffi::OsStr,
) -> Result<u64, String> {
    match raw {
        None => Ok(default),
        Some(value) => value
            .to_str()
            .ok_or_else(|| usage(program))?
            .parse()
            .map_err(|_| format!("{label} must be a non-negative integer; {}", usage(program))),
    }
}

#[cfg(target_os = "linux")]
fn usage(program: &std::ffi::OsStr) -> String {
    format!(
        "usage: {} [size-bytes] [delay-ms] [pasted-file-path]",
        program.to_string_lossy()
    )
}

#[cfg(target_os = "linux")]
struct PatternReader {
    size: u64,
    position: u64,
    delay: Duration,
    reported_first_read: bool,
}

#[cfg(target_os = "linux")]
impl PatternReader {
    fn new(size: u64, delay: Duration) -> Self {
        Self {
            size,
            position: 0,
            delay,
            reported_first_read: false,
        }
    }
}

#[cfg(target_os = "linux")]
impl Read for PatternReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.size {
            return Ok(0);
        }
        if !self.reported_first_read {
            self.reported_first_read = true;
            println!("content_first_read offset={}", self.position);
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

#[cfg(target_os = "linux")]
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
