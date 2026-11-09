use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

/// Host-side limits every `JamPeerTransport` implementation enforces.
pub const JAM_PEER_TRANSPORT_MAX_CONNECTIONS: u32 = 8;
/// Streams one execution may hold open per connection.
pub const JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION: u32 = 16;
/// Largest framed message accepted by `send` or delivered by `recv`.
pub const JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES: u32 = 1 << 20;
/// Bytes the host buffers per connection before applying back-pressure.
pub const JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION: u32 = 4 << 20;
/// Largest fragment carried by `sendChunk` or `recvChunk`.
pub const JAM_PEER_TRANSPORT_MAX_CHUNK_BYTES: u32 = 256 << 10;
/// Largest logical JAMNP-S message available through the streaming API.
pub const JAM_PEER_TRANSPORT_MAX_LOGICAL_MESSAGE_BYTES: u32 = 64 << 20;

/// Failure to dial a JAM peer.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportDialError {
    /// The product holds no `RemotePermission::JamPeers` grant for the
    /// requested genesis, or this host offers no peer transport.
    NotGranted,
    /// The peer refused the connection or presented a certificate that does
    /// not match the requested identity.
    Refused,
    /// The execution's connection budget (including pending dials), or its
    /// eight distinct genesis decisions, is exhausted.
    Limit,
    /// The endpoint could not be reached.
    Unreachable,
}

/// Dial one JAM peer over JAMNP-S (QUIC) or WebTransport.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportDialRequest {
    /// Genesis header hash authorizing this dial. Native QUIC derives its
    /// ALPN from the first four bytes; WebTransport negotiates HTTP/3.
    /// Neither transport authenticates the peer's chain membership.
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
pub struct HostJamPeerTransportDialResponse {
    /// Execution-local connection id.
    pub conn: u32,
}

/// Failure to establish an execution-scoped JAM peer listener.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportListenError {
    /// The genesis grant or separate trusted execution-only consent is absent.
    NotGranted,
    /// The host does not support listeners, or the immutable policy is refused.
    Refused,
    /// The listener or peer admission resource limit is exhausted.
    Limit,
    /// The requested local endpoint could not be bound.
    Unreachable,
}

/// Establish one immutable listener before this execution opens any connection.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportListenRequest {
    /// Genesis covered by `RemotePermission::JamPeers` and the listener consent.
    pub genesis: [u8; 32],
    /// Loopback bind address, IPv6 or IPv4-mapped IPv6.
    pub ip: [u8; 16],
    /// Requested local UDP port.
    pub port: u16,
    /// Only these mutually authenticated TLS Ed25519 identities may connect.
    pub allowed_peers: Vec<[u8; 32]>,
}

/// The bound execution-scoped listener endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportListenResponse {
    /// Bound address, IPv6 or IPv4-mapped IPv6.
    pub ip: [u8; 16],
    /// Bound local UDP port.
    pub port: u16,
}

/// Failure to open a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportOpenError {
    /// This execution has no peer-transport grant.
    NotGranted,
    /// The connection is closed or unknown.
    Closed,
    /// The stream cap for this connection is exhausted.
    Limit,
}

/// Open a bidirectional stream and send its kind byte.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportOpenRequest {
    /// Connection returned by `dial` or reported by `PeerAccepted`.
    pub conn: u32,
    /// JAMNP-S stream kind (UP 0, CE 128, ...).
    pub kind: u8,
}

/// An open stream handle.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportOpenResponse {
    /// Execution-local stream id.
    pub stream: u32,
}

/// Failure to send a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportSendError {
    /// The stream is closed, finished or unknown.
    Closed,
    /// The message exceeds the host's message limit.
    TooLarge,
    /// The per-connection buffer is full.
    Limit,
}

/// Send one framed message; the host adds the `u32` little-endian length.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportSendRequest {
    /// Stream returned by `open` or reported by an `Accepted` event.
    pub stream: u32,
    /// Message bytes without length prefix.
    pub message: Vec<u8>,
    /// Finish the send side after this message. With an empty message, send
    /// only FIN; empty nonterminal messages retain their zero-length prefix.
    pub fin: bool,
}

/// Failure to receive from a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportRecvError {
    /// The stream is unknown or already fully consumed.
    Closed,
}

/// Poll one complete framed message without blocking.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportRecvRequest {
    /// Stream to read from.
    pub stream: u32,
    /// Largest message the caller accepts.
    pub max: u32,
}

/// One unframed message, or none available yet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportRecvResponse {
    /// Complete message bytes without length prefix, or `None` when nothing
    /// has arrived yet.
    pub message: Option<Vec<u8>>,
    /// The peer finished its send side; no further messages will arrive.
    pub fin: bool,
    /// The peer reset the stream; buffered data may be incomplete.
    pub reset: bool,
}

/// Failure to queue a fragment of a single JAMNP-S message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportSendChunkError {
    /// The stream is closed, finished or unknown.
    Closed,
    /// The fragment exceeds 256 KiB or the logical message exceeds 64 MiB.
    TooLarge,
    /// Connection credit is exhausted; retry the same fragment unchanged.
    Limit,
    /// Incorrect offset, changed total, empty nonfinal fragment or premature FIN.
    /// The stream is reset and cannot be reused.
    InvalidChunk,
}

/// Queue one bounded fragment. Only offset zero emits a length prefix.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportSendChunkRequest {
    /// Execution-owned stream.
    pub stream: u32,
    /// Exact logical message length, at most 64 MiB.
    pub total: u32,
    /// Exact next byte offset, beginning at zero.
    pub offset: u32,
    /// At most 256 KiB. Empty is allowed only for a zero-length message.
    pub bytes: Vec<u8>,
    /// Finish only after the final fragment. A zero-length message still frames.
    pub fin: bool,
}

/// Failure to poll a bounded fragment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportRecvChunkError {
    /// The stream is unknown or already fully consumed.
    Closed,
    /// `max_bytes` must be between one and 256 KiB inclusive.
    InvalidMaxBytes,
}

/// Poll a fragment without allocating the complete logical message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportRecvChunkRequest {
    /// Execution-owned stream.
    pub stream: u32,
    /// Maximum fragment bytes, between one and 256 KiB inclusive.
    pub max_bytes: u32,
}

/// A bounded message fragment, or no arrival yet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportRecvChunkResponse {
    /// Declared length, checked before allocation; `None` means no fragment.
    /// `Some(0)` with `complete` represents an actual framed empty message.
    pub total: Option<u32>,
    /// Offset within this logical message (zero when there is no fragment).
    pub offset: u32,
    /// At most the requested maximum; never includes the JAMNP-S length prefix.
    pub bytes: Vec<u8>,
    /// This fragment ends the logical message; false when none is available.
    pub complete: bool,
    /// Clean peer FIN, reported only after all complete messages are drained.
    pub fin: bool,
    /// Peer reset or malformed/truncated framing; discard an incomplete message.
    pub reset: bool,
}

/// Failure to reset a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportResetError {
    /// The stream is unknown or already closed.
    Closed,
}

/// Abort both directions of a stream.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportResetRequest {
    /// Stream to reset.
    pub stream: u32,
}

/// Failure to close a connection.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportCloseError {
    /// The connection is unknown or already closed.
    Closed,
}

/// Close a connection and every stream on it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportCloseRequest {
    /// Connection to close.
    pub conn: u32,
}

/// Failure to drain events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostJamPeerTransportEventsError {
    /// This execution has no peer-transport grant.
    NotGranted,
}

/// Asynchronous transport notification.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum JamPeerTransportEvent {
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
    /// The peer opened a stream on an outgoing or accepted connection.
    Accepted {
        /// Connection the stream arrived on.
        conn: u32,
        /// Execution-local stream id.
        stream: u32,
        /// Stream kind byte the peer sent.
        kind: u8,
    },
    /// A mutually authenticated peer connected to the execution's listener.
    /// This precedes every stream event for the connection.
    PeerAccepted {
        /// Execution-local connection id.
        conn: u32,
        /// Peer identity authenticated by TLS, not inferred from its endpoint.
        ed25519: [u8; 32],
        /// Remote address, IPv6 or IPv4-mapped IPv6.
        ip: [u8; 16],
        /// Remote UDP port.
        port: u16,
    },
}

/// Events in arrival order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostJamPeerTransportEventsResponse {
    /// Pending events; empty when nothing happened.
    pub events: Vec<JamPeerTransportEvent>,
}
