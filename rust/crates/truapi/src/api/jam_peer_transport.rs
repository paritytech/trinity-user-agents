//! Unified [`JamPeerTransport`] trait.

use crate::versioned::jam_peer_transport::{
    HostJamPeerTransportCloseError, HostJamPeerTransportCloseRequest,
    HostJamPeerTransportCloseResponse, HostJamPeerTransportDialError,
    HostJamPeerTransportDialRequest, HostJamPeerTransportDialResponse,
    HostJamPeerTransportEventsError, HostJamPeerTransportEventsRequest,
    HostJamPeerTransportEventsResponse, HostJamPeerTransportOpenError,
    HostJamPeerTransportOpenRequest, HostJamPeerTransportOpenResponse,
    HostJamPeerTransportRecvError, HostJamPeerTransportRecvRequest,
    HostJamPeerTransportRecvResponse, HostJamPeerTransportResetError,
    HostJamPeerTransportResetRequest, HostJamPeerTransportResetResponse,
    HostJamPeerTransportSendError, HostJamPeerTransportSendRequest,
    HostJamPeerTransportSendResponse,
    HostJamPeerTransportListenError, HostJamPeerTransportListenRequest,
    HostJamPeerTransportListenResponse,
    HostJamPeerTransportSendChunkError, HostJamPeerTransportSendChunkRequest,
    HostJamPeerTransportSendChunkResponse, HostJamPeerTransportRecvChunkError,
    HostJamPeerTransportRecvChunkRequest, HostJamPeerTransportRecvChunkResponse,
};
use crate::{CallContext, CallError, v01, wire, wire_trait};

/// Host-terminated QUIC/WebTransport streams to JAM peers (JAMNP-S).
///
/// The host owns TLS, certificate verification and length framing; the guest
/// verifies every byte it consumes. Access is a runtime permission, not a
/// manifest declaration: `dial` requires
/// [`RemotePermission::JamPeers`](crate::v01::RemotePermission::JamPeers) for
/// its `genesis`, checking the product's stored decision, prompting when it is
/// undetermined and persisting the answer per product and genesis. `listen`
/// additionally requires separate trusted, execution-only consent for the exact
/// policy and custodied identity; a stored outbound grant never authorizes it.
/// Other methods act only on this execution's outgoing or accepted connections.
/// A grant is separate from account, signing and storage authority.
#[wire_trait(id = 111)]
#[crate::async_trait]
pub trait JamPeerTransport: Send + Sync {
    /// Dial one peer. Native QUIC builds the ALPN from `genesis` and requires
    /// the peer certificate to carry `ed25519`. WebTransport negotiates
    /// HTTP/3 and pins the certificate derived from `p256`. These checks
    /// authenticate the caller-supplied peer identity, not chain membership.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.dial({
    ///   genesis: "0x353963b9cedfe4ea22038081052a5c151b06b55a4a026a97522cd0320cabf49f",
    ///   ip: "0x00000000000000000000ffff7f000001",
    ///   port: 43000,
    ///   ed25519: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///   p256: undefined,
    /// });
    /// if (result.isOk()) console.log("connection:", result.value.conn);
    /// ```
    #[wire(id = 0)]
    async fn dial(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportDialRequest,
    ) -> Result<HostJamPeerTransportDialResponse, CallError<HostJamPeerTransportDialError>> {
        Err(CallError::Domain(HostJamPeerTransportDialError::V1(
            v01::HostJamPeerTransportDialError::NotGranted,
        )))
    }

    /// Open a bidirectional stream on a connection and send its kind byte.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.open({ conn: 0, kind: 0 });
    /// if (result.isOk()) console.log("stream:", result.value.stream);
    /// ```
    #[wire(id = 1)]
    async fn open(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportOpenRequest,
    ) -> Result<HostJamPeerTransportOpenResponse, CallError<HostJamPeerTransportOpenError>> {
        Err(CallError::Domain(HostJamPeerTransportOpenError::V1(
            v01::HostJamPeerTransportOpenError::NotGranted,
        )))
    }

    /// Queue one message with a `u32` little-endian length prefix. An empty
    /// message with `fin: true` sends only FIN, without a zero-length frame.
    /// An empty nonterminal message still sends the zero-length prefix.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.send({ stream: 0, message: "0x00", fin: false });
    /// console.log("sent:", result.isOk());
    /// ```
    #[wire(id = 2)]
    async fn send(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportSendRequest,
    ) -> Result<HostJamPeerTransportSendResponse, CallError<HostJamPeerTransportSendError>> {
        Err(CallError::Domain(HostJamPeerTransportSendError::V1(
            v01::HostJamPeerTransportSendError::Closed,
        )))
    }

    /// Poll one complete message without blocking; the host strips the length.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.recv({ stream: 0, max: 1048576 });
    /// if (result.isOk()) console.log("message:", result.value.message, "fin:", result.value.fin);
    /// ```
    #[wire(id = 3)]
    async fn recv(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportRecvRequest,
    ) -> Result<HostJamPeerTransportRecvResponse, CallError<HostJamPeerTransportRecvError>> {
        Err(CallError::Domain(HostJamPeerTransportRecvError::V1(
            v01::HostJamPeerTransportRecvError::Closed,
        )))
    }

    /// Abort a stream in both directions.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.reset({ stream: 0 });
    /// console.log("reset:", result.isOk());
    /// ```
    #[wire(id = 4)]
    async fn reset(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportResetRequest,
    ) -> Result<HostJamPeerTransportResetResponse, CallError<HostJamPeerTransportResetError>> {
        Err(CallError::Domain(HostJamPeerTransportResetError::V1(
            v01::HostJamPeerTransportResetError::Closed,
        )))
    }

    /// Close a connection and every stream on it.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.close({ conn: 0 });
    /// console.log("closed:", result.isOk());
    /// ```
    #[wire(id = 5)]
    async fn close(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportCloseRequest,
    ) -> Result<HostJamPeerTransportCloseResponse, CallError<HostJamPeerTransportCloseError>> {
        Err(CallError::Domain(HostJamPeerTransportCloseError::V1(
            v01::HostJamPeerTransportCloseError::Closed,
        )))
    }

    /// Drain connection, stream-finish and inbound-stream events.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.events();
    /// if (result.isOk()) console.log("events:", result.value.events);
    /// ```
    #[wire(id = 6)]
    async fn events(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportEventsRequest,
    ) -> Result<HostJamPeerTransportEventsResponse, CallError<HostJamPeerTransportEventsError>>
    {
        Err(CallError::Domain(HostJamPeerTransportEventsError::V1(
            v01::HostJamPeerTransportEventsError::NotGranted,
        )))
    }

    /// Listen using the execution's already acquired custody identity.
    ///
    /// Experimental desktop hosts admit only loopback. The trusted confirmation
    /// names product, artifact, custodied public key, full genesis, bind endpoint
    /// and allowed peers. Configure one immutable policy before any connection;
    /// attempts to change or rebind it fail rather than replacing live resources.
    /// Cancellation, revocation and disposal fence accepts and drain the listener
    /// and connections before custody ownership is released. Unsupported hosts
    /// explicitly refuse.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.listen({
    ///   genesis: "0x353963b9cedfe4ea22038081052a5c151b06b55a4a026a97522cd0320cabf49f",
    ///   ip: "0x00000000000000000000ffff7f000001",
    ///   port: 43008,
    ///   allowedPeers: ["0xb4332c622fb1fc43a3cf083b7fdefa7e75bf3715c426830c2f397150ffa6dc0b"],
    /// });
    /// if (result.isOk()) console.log("bound port:", result.value.port);
    /// ```
    #[wire(id = 7)]
    async fn listen(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportListenRequest,
    ) -> Result<HostJamPeerTransportListenResponse, CallError<HostJamPeerTransportListenError>> {
        Err(CallError::Domain(HostJamPeerTransportListenError::V1(
            v01::HostJamPeerTransportListenError::Refused,
        )))
    }

    /// Queue a fragment of one logical JAMNP-S message (at most 64 MiB).
    /// Fragments are at most 256 KiB, ordered, contiguous and not interleaved
    /// with `send`. `Limit` leaves the next offset unchanged. Malformed
    /// sequences reset the stream; FIN is allowed only on the final fragment.
    ///
    /// ```ts
    /// // Use an owned stream; offsets continue the same declared logical message.
    /// const result = await truapi.jamPeerTransport.sendChunk({
    ///   stream: 1, total: 3, offset: 0, bytes: "0x010203", fin: true,
    /// });
    /// console.log("fragment accepted:", result.isOk());
    /// ```
    #[wire(id = 8)]
    async fn send_chunk(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportSendChunkRequest,
    ) -> Result<HostJamPeerTransportSendChunkResponse, CallError<HostJamPeerTransportSendChunkError>> {
        Err(CallError::Unsupported)
    }

    /// Poll up to 256 KiB of a message without whole-message allocation.
    /// Includes already prefetched bytes. `total: Some(0)` is an empty message,
    /// unlike `None` (no arrival). Reset invalidates any incomplete message.
    /// Do not switch to `recv` in the middle of a logical message.
    ///
    /// ```ts
    /// const result = await truapi.jamPeerTransport.recvChunk({
    ///   stream: 1, maxBytes: 262144,
    /// });
    /// if (result.isOk()) console.log("message complete:", result.value.complete);
    /// ```
    #[wire(id = 9)]
    async fn recv_chunk(
        &self,
        _cx: &CallContext,
        _request: HostJamPeerTransportRecvChunkRequest,
    ) -> Result<HostJamPeerTransportRecvChunkResponse, CallError<HostJamPeerTransportRecvChunkError>> {
        Err(CallError::Unsupported)
    }
}
