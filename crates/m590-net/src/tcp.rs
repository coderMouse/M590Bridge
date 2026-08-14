//! TCP transport for length-prefixed protocol frames (std only, no async runtime).

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use m590_core::Message;

use crate::frame::{encode_frame, try_decode_frame, FrameError, FRAME_HEADER_LEN, MAX_PAYLOAD_LEN};

const SOCKET_READ_CHUNK_LEN: usize = 64 * 1024;
const MAX_BUFFERED_LEN: usize = FRAME_HEADER_LEN + MAX_PAYLOAD_LEN + SOCKET_READ_CHUNK_LEN;

/// Errors from TCP framed I/O.
#[derive(Debug)]
pub enum TcpError {
    Io(io::Error),
    Frame(FrameError),
    InvalidAddr(String),
    Disconnected,
}

impl std::fmt::Display for TcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "tcp io error: {err}"),
            Self::Frame(err) => write!(f, "tcp frame error: {err}"),
            Self::InvalidAddr(msg) => write!(f, "invalid address: {msg}"),
            Self::Disconnected => write!(f, "peer disconnected"),
        }
    }
}

impl std::error::Error for TcpError {}

impl From<io::Error> for TcpError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FrameError> for TcpError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}

/// Bidirectional TCP stream that speaks M590 frames.
pub struct TcpFrameStream {
    stream: TcpStream,
    buffer: Vec<u8>,
    read_chunk: Box<[u8]>,
}

impl TcpFrameStream {
    pub fn from_stream(stream: TcpStream) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            buffer: Vec::with_capacity(SOCKET_READ_CHUNK_LEN),
            read_chunk: vec![0u8; SOCKET_READ_CHUNK_LEN].into_boxed_slice(),
        })
    }

    pub fn peer_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.stream.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.stream.local_addr()
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.stream.set_nonblocking(nonblocking)
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.stream.set_read_timeout(timeout)
    }

    pub fn send(&mut self, message: &Message) -> Result<(), TcpError> {
        let bytes = encode_frame(message)?;
        self.write_all_blocking(&bytes)
    }

    pub fn send_all<'a, I>(&mut self, messages: I) -> Result<(), TcpError>
    where
        I: IntoIterator<Item = &'a Message>,
    {
        for message in messages {
            self.send(message)?;
        }
        Ok(())
    }

    /// Write the full buffer in blocking mode.
    ///
    /// `try_recv` switches the socket to non-blocking. Without restoring
    /// blocking mode, a multi-megabyte `ClipboardImage` frame can fail with
    /// `WouldBlock` (Linux os error 11) and the hub treats it as disconnect.
    fn write_all_blocking(&mut self, mut bytes: &[u8]) -> Result<(), TcpError> {
        self.set_nonblocking(false)?;
        self.stream
            .set_write_timeout(Some(Duration::from_secs(60)))
            .ok();
        while !bytes.is_empty() {
            match self.stream.write(bytes) {
                Ok(0) => {
                    return Err(TcpError::Io(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "tcp write returned 0",
                    )));
                }
                Ok(n) => bytes = &bytes[n..],
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err)
                    if err.kind() == io::ErrorKind::WouldBlock
                        || err.kind() == io::ErrorKind::TimedOut =>
                {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(err) => return Err(TcpError::Io(err)),
            }
        }
        self.stream.flush()?;
        Ok(())
    }

    /// Blocking read of the next complete frame.
    pub fn recv(&mut self) -> Result<Message, TcpError> {
        self.set_nonblocking(false)?;
        loop {
            if let Some(msg) = self.try_decode_buffered()? {
                return Ok(msg);
            }
            self.read_more_blocking()?;
        }
    }

    /// Non-blocking style: return `Ok(None)` if no complete frame is ready yet.
    pub fn try_recv(&mut self) -> Result<Option<Message>, TcpError> {
        self.set_nonblocking(true)?;
        if let Some(msg) = self.try_decode_buffered()? {
            return Ok(Some(msg));
        }

        loop {
            if !self.read_more_nonblocking_once()? {
                return Ok(None);
            }
            if let Some(msg) = self.try_decode_buffered()? {
                return Ok(Some(msg));
            }
        }
    }

    fn try_decode_buffered(&mut self) -> Result<Option<Message>, TcpError> {
        match try_decode_frame(&self.buffer)? {
            None => Ok(None),
            Some((msg, consumed)) => {
                self.buffer.drain(..consumed);
                Ok(Some(msg))
            }
        }
    }

    fn read_more_blocking(&mut self) -> Result<(), TcpError> {
        let n = self.stream.read(&mut self.read_chunk)?;
        if n == 0 {
            return Err(TcpError::Disconnected);
        }
        self.buffer.extend_from_slice(&self.read_chunk[..n]);
        if self.buffer.len() > MAX_BUFFERED_LEN {
            return Err(TcpError::Frame(FrameError::PayloadTooLarge(
                self.buffer.len(),
            )));
        }
        Ok(())
    }

    fn read_more_nonblocking_once(&mut self) -> Result<bool, TcpError> {
        match self.stream.read(&mut self.read_chunk) {
            Ok(0) => Err(TcpError::Disconnected),
            Ok(n) => {
                self.buffer.extend_from_slice(&self.read_chunk[..n]);
                if self.buffer.len() > MAX_BUFFERED_LEN {
                    return Err(TcpError::Frame(FrameError::PayloadTooLarge(
                        self.buffer.len(),
                    )));
                }
                Ok(true)
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(err) if err.kind() == io::ErrorKind::TimedOut => Ok(false),
            Err(err) => Err(TcpError::Io(err)),
        }
    }
}

/// Bind a TCP listener (dual-stack as provided by OS default).
pub fn listen_on(addr: impl ToSocketAddrs) -> Result<TcpListener, TcpError> {
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(false)?;
    Ok(listener)
}

/// Accept one client and wrap as framed stream.
pub fn accept_framed(listener: &TcpListener) -> Result<TcpFrameStream, TcpError> {
    let (stream, _peer) = listener.accept()?;
    Ok(TcpFrameStream::from_stream(stream)?)
}

/// Dial a remote framed peer.
pub fn connect_framed(addr: impl ToSocketAddrs) -> Result<TcpFrameStream, TcpError> {
    let stream = TcpStream::connect(addr)?;
    Ok(TcpFrameStream::from_stream(stream)?)
}

/// Dial a remote framed peer without allowing one OS connect call to block forever.
pub fn connect_framed_timeout(
    addr: impl ToSocketAddrs,
    timeout: Duration,
) -> Result<TcpFrameStream, TcpError> {
    if timeout.is_zero() {
        return Err(TcpError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "tcp connect timeout",
        )));
    }
    let addrs = addr
        .to_socket_addrs()
        .map_err(|e| TcpError::InvalidAddr(e.to_string()))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(TcpError::InvalidAddr("could not resolve address".into()));
    }

    let deadline = Instant::now() + timeout;
    let mut last_error = None;
    for socket_addr in addrs {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(&socket_addr, remaining) {
            Ok(stream) => return Ok(TcpFrameStream::from_stream(stream)?),
            Err(err) => last_error = Some(err),
        }
    }
    Err(TcpError::Io(last_error.unwrap_or_else(|| {
        io::Error::new(io::ErrorKind::TimedOut, "tcp connect timeout")
    })))
}

/// Parse `host:port` (IPv4/hostname). IPv6 with brackets not required for MVP.
pub fn parse_socket_addr(input: &str) -> Result<std::net::SocketAddr, TcpError> {
    input
        .to_socket_addrs()
        .map_err(|e| TcpError::InvalidAddr(e.to_string()))?
        .next()
        .ok_or_else(|| TcpError::InvalidAddr(format!("could not resolve {input}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use m590_core::{
        ClipboardTextPayload, ConnectionState, DeviceId, FileChunkPayload, Message, Session,
        SessionEvent,
    };
    use std::thread;
    use std::time::{Duration, Instant};

    fn exchange_until_connected(
        session: &mut Session,
        conn: &mut TcpFrameStream,
        label: &str,
        deadline: Instant,
    ) {
        while session.state() != ConnectionState::Connected {
            assert!(
                Instant::now() < deadline,
                "{label} pairing timeout state={:?}",
                session.state()
            );
            conn.set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            match conn.recv() {
                Ok(msg) => {
                    session.handle(SessionEvent::Message(msg)).unwrap();
                    let out = session.take_outbox();
                    conn.send_all(out.iter()).unwrap();
                }
                Err(TcpError::Io(err))
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(err) => panic!("{label} recv failed: {err}"),
            }
        }
    }

    #[test]
    fn tcp_loopback_pairs_and_syncs_clipboard_text() {
        let listener = listen_on("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let mut host_conn = accept_framed(&listener).unwrap();
            let mut host = Session::new(DeviceId::new("host")).unwrap();
            host.handle(SessionEvent::StartPairing {
                expected_code: "999888".into(),
            })
            .unwrap();
            let _ = host.take_outbox();

            let deadline = Instant::now() + Duration::from_secs(3);
            exchange_until_connected(&mut host, &mut host_conn, "host", deadline);

            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                assert!(Instant::now() < deadline, "host clipboard timeout");
                host_conn
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                match host_conn.recv() {
                    Ok(msg) => {
                        host.handle(SessionEvent::Message(msg)).unwrap();
                        host_conn.send_all(host.take_outbox().iter()).unwrap();
                        if host.snapshot().last_clipboard_text.as_deref() == Some("tcp-hello") {
                            break;
                        }
                    }
                    Err(TcpError::Io(err))
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(err) => panic!("host clipboard recv failed: {err}"),
                }
            }
            host.snapshot().last_clipboard_text
        });

        // Ensure accept is ready.
        thread::sleep(Duration::from_millis(100));
        let mut joiner_conn = connect_framed(addr).unwrap();
        let mut joiner = Session::new(DeviceId::new("joiner")).unwrap();
        joiner
            .handle(SessionEvent::StartPairing {
                expected_code: "999888".into(),
            })
            .unwrap();
        let out = joiner.take_outbox();
        assert!(out.len() >= 2, "joiner should emit hello+pair_request");
        joiner_conn.send_all(out.iter()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        exchange_until_connected(&mut joiner, &mut joiner_conn, "joiner", deadline);

        joiner.queue_clipboard_text("cid-tcp", "tcp-hello").unwrap();
        joiner_conn.send_all(joiner.take_outbox().iter()).unwrap();

        let got = server.join().expect("host thread panicked");
        assert_eq!(got.as_deref(), Some("tcp-hello"));
    }

    #[test]
    fn tcp_frame_roundtrip_single_message() {
        let listener = listen_on("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut conn = accept_framed(&listener).unwrap();
            conn.recv().unwrap()
        });
        thread::sleep(Duration::from_millis(20));
        let mut client = connect_framed(addr).unwrap();
        let msg = Message::clipboard_text(
            ClipboardTextPayload::new(DeviceId::new("a"), "c", "ping").unwrap(),
        );
        client.send(&msg).unwrap();
        assert_eq!(server.join().unwrap(), msg);
    }

    #[test]
    fn tcp_connect_timeout_entry_connects_to_loopback() {
        let listener = listen_on("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || accept_framed(&listener).unwrap().peer_addr().unwrap());

        let client = connect_framed_timeout(addr, Duration::from_secs(1)).unwrap();
        assert_eq!(client.peer_addr().unwrap(), addr);
        assert_eq!(server.join().unwrap(), client.local_addr().unwrap());
    }

    #[test]
    fn tcp_connect_timeout_rejects_zero_duration() {
        let err = match connect_framed_timeout("127.0.0.1:9", Duration::ZERO) {
            Ok(_) => panic!("zero timeout must fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("connect timeout"), "{err}");
    }

    #[test]
    fn tcp_loopback_sends_large_image_after_try_recv() {
        use m590_core::ClipboardImagePayload;

        let listener = listen_on("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut conn = accept_framed(&listener).unwrap();
            // Mimic hub: non-blocking poll first (leaves socket non-blocking).
            let _ = conn.try_recv();
            conn.recv().unwrap()
        });
        thread::sleep(Duration::from_millis(20));
        let mut client = connect_framed(addr).unwrap();
        // Leave client in the same state hub uses between polls.
        let _ = client.try_recv();

        // ~1.2MiB raw RGBA — large enough to stress non-blocking write buffers.
        let width = 600u32;
        let height = 500u32;
        let rgba = vec![7u8; (width as usize) * (height as usize) * 4];
        let msg = Message::clipboard_image(
            ClipboardImagePayload::new(DeviceId::new("cam"), "big-img", width, height, rgba)
                .unwrap(),
        );
        client
            .send(&msg)
            .expect("large image send must not fail with WouldBlock after try_recv");
        let got = server.join().unwrap();
        assert_eq!(got, msg);
    }

    #[test]
    fn tcp_nonblocking_decodes_many_frames_exceeding_single_frame_limit() {
        const CHUNK_LEN: usize = 256 * 1024;
        const FRAME_COUNT: usize = MAX_PAYLOAD_LEN / CHUNK_LEN + 8;

        let listener = listen_on("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut conn = accept_framed(&listener).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut received = 0usize;
            while received < FRAME_COUNT {
                assert!(Instant::now() < deadline, "bulk frame receive timeout");
                match conn.try_recv() {
                    Ok(Some(Message::FileChunk(payload))) => {
                        assert_eq!(payload.offset, (received * CHUNK_LEN) as u64);
                        assert_eq!(payload.data.len(), CHUNK_LEN);
                        received += 1;
                    }
                    Ok(Some(other)) => panic!("unexpected message: {}", other.name()),
                    Ok(None) => thread::yield_now(),
                    Err(err) => panic!("bulk frame receive failed: {err}"),
                }
            }
            received
        });

        let mut client = connect_framed(addr).unwrap();
        for index in 0..FRAME_COUNT {
            let payload = FileChunkPayload::new(
                DeviceId::new("bulk-sender"),
                "bulk-transfer",
                (index * CHUNK_LEN) as u64,
                vec![index as u8; CHUNK_LEN],
            )
            .unwrap();
            client.send(&Message::file_chunk(payload)).unwrap();
        }

        assert_eq!(server.join().unwrap(), FRAME_COUNT);
    }
}
