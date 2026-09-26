//! Unified [`PeerTransport`] trait.

use crate::versioned::peer_transport::{
    HostPeerTransportCloseError, HostPeerTransportCloseRequest, HostPeerTransportCloseResponse,
    HostPeerTransportDialError, HostPeerTransportDialRequest, HostPeerTransportDialResponse,
    HostPeerTransportEventsError, HostPeerTransportEventsRequest, HostPeerTransportEventsResponse,
    HostPeerTransportOpenError, HostPeerTransportOpenRequest, HostPeerTransportOpenResponse,
    HostPeerTransportRecvError, HostPeerTransportRecvRequest, HostPeerTransportRecvResponse,
    HostPeerTransportResetError, HostPeerTransportResetRequest, HostPeerTransportResetResponse,
    HostPeerTransportSendError, HostPeerTransportSendRequest, HostPeerTransportSendResponse,
};
use crate::{CallContext, CallError, v01, wire, wire_trait};

/// Host-terminated QUIC/WebTransport streams to JAM peers (JAMNP-S).
///
/// The host owns TLS, certificate verification and length framing; the guest
/// verifies every byte it consumes. Access requires the manifest capability
/// `capabilities.network.jam = { genesis }` and is granted only for that
/// genesis. A grant is separate from account, signing and storage authority.
#[wire_trait(id = 21)]
#[crate::async_trait]
pub trait PeerTransport: Send + Sync {
    /// Dial one peer. The host builds the ALPN from `genesis` and requires the
    /// peer certificate to carry `ed25519` (QUIC) or to hash to the
    /// certificate derived from `p256` (WebTransport).
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.dial({
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
        _request: HostPeerTransportDialRequest,
    ) -> Result<HostPeerTransportDialResponse, CallError<HostPeerTransportDialError>> {
        Err(CallError::Domain(HostPeerTransportDialError::V1(
            v01::HostPeerTransportDialError::NotGranted,
        )))
    }

    /// Open a bidirectional stream on a connection and send its kind byte.
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.open({ conn: 0, kind: 0 });
    /// if (result.isOk()) console.log("stream:", result.value.stream);
    /// ```
    #[wire(id = 1)]
    async fn open(
        &self,
        _cx: &CallContext,
        _request: HostPeerTransportOpenRequest,
    ) -> Result<HostPeerTransportOpenResponse, CallError<HostPeerTransportOpenError>> {
        Err(CallError::Domain(HostPeerTransportOpenError::V1(
            v01::HostPeerTransportOpenError::NotGranted,
        )))
    }

    /// Queue one message; the host prepends the `u32` little-endian length.
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.send({ stream: 0, message: "0x00", fin: false });
    /// console.log("sent:", result.isOk());
    /// ```
    #[wire(id = 2)]
    async fn send(
        &self,
        _cx: &CallContext,
        _request: HostPeerTransportSendRequest,
    ) -> Result<HostPeerTransportSendResponse, CallError<HostPeerTransportSendError>> {
        Err(CallError::Domain(HostPeerTransportSendError::V1(
            v01::HostPeerTransportSendError::Closed,
        )))
    }

    /// Poll one complete message without blocking; the host strips the length.
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.recv({ stream: 0, max: 1048576 });
    /// if (result.isOk()) console.log("message:", result.value.message, "fin:", result.value.fin);
    /// ```
    #[wire(id = 3)]
    async fn recv(
        &self,
        _cx: &CallContext,
        _request: HostPeerTransportRecvRequest,
    ) -> Result<HostPeerTransportRecvResponse, CallError<HostPeerTransportRecvError>> {
        Err(CallError::Domain(HostPeerTransportRecvError::V1(
            v01::HostPeerTransportRecvError::Closed,
        )))
    }

    /// Abort a stream in both directions.
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.reset({ stream: 0 });
    /// console.log("reset:", result.isOk());
    /// ```
    #[wire(id = 4)]
    async fn reset(
        &self,
        _cx: &CallContext,
        _request: HostPeerTransportResetRequest,
    ) -> Result<HostPeerTransportResetResponse, CallError<HostPeerTransportResetError>> {
        Err(CallError::Domain(HostPeerTransportResetError::V1(
            v01::HostPeerTransportResetError::Closed,
        )))
    }

    /// Close a connection and every stream on it.
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.close({ conn: 0 });
    /// console.log("closed:", result.isOk());
    /// ```
    #[wire(id = 5)]
    async fn close(
        &self,
        _cx: &CallContext,
        _request: HostPeerTransportCloseRequest,
    ) -> Result<HostPeerTransportCloseResponse, CallError<HostPeerTransportCloseError>> {
        Err(CallError::Domain(HostPeerTransportCloseError::V1(
            v01::HostPeerTransportCloseError::Closed,
        )))
    }

    /// Drain connection, stream-finish and inbound-stream events.
    ///
    /// ```ts
    /// const result = await truapi.peerTransport.events();
    /// if (result.isOk()) console.log("events:", result.value.events);
    /// ```
    #[wire(id = 6)]
    async fn events(
        &self,
        _cx: &CallContext,
        _request: HostPeerTransportEventsRequest,
    ) -> Result<HostPeerTransportEventsResponse, CallError<HostPeerTransportEventsError>> {
        Err(CallError::Domain(HostPeerTransportEventsError::V1(
            v01::HostPeerTransportEventsError::NotGranted,
        )))
    }
}
