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
};
use crate::{CallContext, CallError, v01, wire, wire_trait};

/// Host-terminated QUIC/WebTransport streams to JAM peers (JAMNP-S).
///
/// The host owns TLS, certificate verification and length framing; the guest
/// verifies every byte it consumes. Access is a runtime permission, not a
/// manifest declaration: `dial` requires
/// [`RemotePermission::JamPeers`](crate::v01::RemotePermission::JamPeers) for
/// its `genesis`, checking the product's stored decision, prompting when it is
/// undetermined and persisting the answer per product and genesis. The other
/// methods act only on connections a granted `dial` opened. A grant is
/// separate from account, signing and storage authority.
#[wire_trait(id = 23)]
#[crate::async_trait]
pub trait JamPeerTransport: Send + Sync {
    /// Dial one peer. The host builds the ALPN from `genesis` and requires the
    /// peer certificate to carry `ed25519` (QUIC) or to hash to the
    /// certificate derived from `p256` (WebTransport).
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

    /// Queue one message; the host prepends the `u32` little-endian length.
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
}
