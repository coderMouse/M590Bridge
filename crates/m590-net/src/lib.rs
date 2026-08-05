//! LAN transport: frame codec, memory pipe, and TCP framed streams.

mod frame;
mod pipe;
mod tcp;

pub use frame::{
    decode_frame, encode_frame, try_decode_frame, FrameError, FRAME_HEADER_LEN, FRAME_MAGIC,
};
pub use pipe::{deliver, DeliverError, MemoryPipe};
pub use tcp::{
    accept_framed, connect_framed, listen_on, parse_socket_addr, TcpError, TcpFrameStream,
};

use m590_core::ConnectionState;

/// Default listen port placeholder (open-question Q3; not locked for production).
pub const DEFAULT_PORT: u16 = 5901;

/// Endpoint description for future manual IP + code pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAddr {
    pub host: String,
    pub port: u16,
}

impl PeerAddr {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub fn with_default_port(host: impl Into<String>) -> Self {
        Self::new(host, DEFAULT_PORT)
    }

    pub fn to_string_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// No-op transport used when no peer link exists yet.
#[derive(Debug, Default)]
pub struct NullTransport;

impl NullTransport {
    pub fn connection_state(&self) -> ConnectionState {
        ConnectionState::Disconnected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m590_core::{ClipboardTextPayload, DeviceId, Message, PROTOCOL_VERSION};

    #[test]
    fn peer_addr_default_port() {
        let addr = PeerAddr::with_default_port("192.0.2.10");
        assert_eq!(addr.port, DEFAULT_PORT);
        assert_eq!(addr.to_string_addr(), "192.0.2.10:5901");
    }

    #[test]
    fn null_transport_is_disconnected() {
        assert_eq!(
            NullTransport.connection_state(),
            ConnectionState::Disconnected
        );
    }

    #[test]
    fn frame_roundtrip_all_message_kinds() {
        let samples = vec![
            Message::hello(DeviceId::new("a"), "0.1.0").unwrap(),
            Message::hello_ack(DeviceId::new("b"), "0.1.0").unwrap(),
            Message::pair_request(DeviceId::new("a"), "123456").unwrap(),
            Message::pair_accept(DeviceId::new("b")).unwrap(),
            Message::pair_reject(DeviceId::new("b"), "nope").unwrap(),
            Message::heartbeat(7),
            Message::heartbeat_ack(7),
            Message::clipboard_text(
                ClipboardTextPayload::new(DeviceId::new("a"), "cid-1", "你好 clipboard").unwrap(),
            ),
            Message::clipboard_image(
                m590_core::ClipboardImagePayload::new(
                    DeviceId::new("a"),
                    "img-1",
                    1,
                    1,
                    vec![10, 20, 30, 255],
                )
                .unwrap(),
            ),
            Message::file_offer(
                m590_core::FileOfferPayload::new(DeviceId::new("a"), "t1", "f.bin", 2).unwrap(),
            ),
            Message::file_request(
                m590_core::FileRequestPayload::new(DeviceId::new("b"), "t1").unwrap(),
            ),
            Message::file_chunk(
                m590_core::FileChunkPayload::new(DeviceId::new("a"), "t1", 0, b"hi".to_vec())
                    .unwrap(),
            ),
            Message::file_complete(
                m590_core::FileCompletePayload::new(DeviceId::new("a"), "t1", true, "").unwrap(),
            ),
            Message::goodbye(DeviceId::new("a"), "bye").unwrap(),
        ];

        for msg in samples {
            let bytes = encode_frame(&msg).unwrap();
            assert_eq!(&bytes[..4], FRAME_MAGIC);
            assert_eq!(bytes[4], PROTOCOL_VERSION);
            let decoded = decode_frame(&bytes).unwrap();
            assert_eq!(decoded, msg);
            let (again, n) = try_decode_frame(&bytes).unwrap().unwrap();
            assert_eq!(again, msg);
            assert_eq!(n, bytes.len());
        }
    }
}
