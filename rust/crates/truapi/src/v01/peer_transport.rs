use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

/// Host-side limits every `PeerTransport` implementation enforces.
pub const PEER_TRANSPORT_MAX_CONNECTIONS: u32 = 8;
/// Streams one execution may hold open per connection.
pub const PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION: u32 = 16;
/// Largest framed message accepted by `send` or delivered by `recv`.
pub const PEER_TRANSPORT_MAX_MESSAGE_BYTES: u32 = 1 << 20;
/// Bytes the host buffers per connection before applying back-pressure.
pub const PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION: u32 = 4 << 20;

/// Failure to dial a JAM peer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportDialError {
    /// This execution has no peer-transport grant for the requested genesis.
    NotGranted,
    /// The peer refused the connection or presented a certificate that does
    /// not match the requested identity.
    Refused,
    /// The connection cap for this execution is exhausted.
    Limit,
    /// The endpoint could not be reached.
    Unreachable,
}

/// Dial one JAM peer over JAMNP-S (QUIC) or WebTransport.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportDialRequest {
    /// Genesis header hash; the host derives the ALPN from it and requires a
    /// matching manifest grant.
    pub genesis: [u8; 32],
    /// Peer IP address, IPv6 or v4-mapped IPv6.
    pub ip: [u8; 16],
    /// Peer UDP port.
    pub port: u16,
    /// Ed25519 key the peer's TLS certificate must carry.
    pub ed25519: [u8; 32],
    /// Compressed P-256 peer key for WebTransport certificate hashes.
    pub p256: Option<[u8; 33]>,
}

/// An open connection handle.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportDialResponse {
    /// Execution-local connection id.
    pub conn: u32,
}

/// Failure to open a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportOpenError {
    /// This execution has no peer-transport grant.
    NotGranted,
    /// The connection is closed or unknown.
    Closed,
    /// The stream cap for this connection is exhausted.
    Limit,
}

/// Open a bidirectional stream and send its kind byte.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportOpenRequest {
    /// Connection returned by `dial`.
    pub conn: u32,
    /// JAMNP-S stream kind (UP 0, CE 128, ...).
    pub kind: u8,
}

/// An open stream handle.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportOpenResponse {
    /// Execution-local stream id.
    pub stream: u32,
}

/// Failure to send a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportSendError {
    /// The stream is closed, finished or unknown.
    Closed,
    /// The message exceeds the host's message limit.
    TooLarge,
    /// The per-connection buffer is full.
    Limit,
}

/// Send one framed message; the host adds the `u32` little-endian length.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportSendRequest {
    /// Stream returned by `open` or reported by an `Accepted` event.
    pub stream: u32,
    /// Message bytes without length prefix.
    pub message: Vec<u8>,
    /// Finish the send side after this message.
    pub fin: bool,
}

/// Failure to receive from a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportRecvError {
    /// The stream is unknown or already fully consumed.
    Closed,
}

/// Poll one complete framed message without blocking.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportRecvRequest {
    /// Stream to read from.
    pub stream: u32,
    /// Largest message the caller accepts.
    pub max: u32,
}

/// One unframed message, or none available yet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportRecvResponse {
    /// Complete message bytes without length prefix, or `None` when nothing
    /// has arrived yet.
    pub message: Option<Vec<u8>>,
    /// The peer finished its send side; no further messages will arrive.
    pub fin: bool,
    /// The peer reset the stream; buffered data may be incomplete.
    pub reset: bool,
}

/// Failure to reset a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportResetError {
    /// The stream is unknown or already closed.
    Closed,
}

/// Abort both directions of a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportResetRequest {
    /// Stream to reset.
    pub stream: u32,
}

/// Failure to close a connection.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportCloseError {
    /// The connection is unknown or already closed.
    Closed,
}

/// Close a connection and every stream on it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportCloseRequest {
    /// Connection to close.
    pub conn: u32,
}

/// Failure to drain events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPeerTransportEventsError {
    /// This execution has no peer-transport grant.
    NotGranted,
}

/// Asynchronous transport notification.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PeerTransportEvent {
    /// The connection was closed by the peer or the host.
    ConnClosed {
        /// Connection that closed.
        conn: u32,
    },
    /// The peer finished its send side of a stream.
    StreamFin {
        /// Stream that finished.
        stream: u32,
    },
    /// The peer opened a stream to us on a dialed connection.
    Accepted {
        /// Connection the stream arrived on.
        conn: u32,
        /// Execution-local stream id.
        stream: u32,
        /// Stream kind byte the peer sent.
        kind: u8,
    },
}

/// Events in arrival order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPeerTransportEventsResponse {
    /// Pending events; empty when nothing happened.
    pub events: Vec<PeerTransportEvent>,
}
