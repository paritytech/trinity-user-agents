//! Product-facing JAM peer transport.
//!
//! Native runtimes dial JAMNP-S QUIC through the connection's
//! [`JamPeerSession`](crate::jam_peer_transport::session::JamPeerSession),
//! gating every dial on [`ProductRuntimeHost::require_jam_peers`]. The browser
//! core keeps the trait's `NotGranted` defaults: its JavaScript session answers
//! trait 111 before frames reach the core.

use crate::runtime::ProductRuntimeHost;

#[cfg(target_arch = "wasm32")]
#[truapi::async_trait]
impl truapi::api::JamPeerTransport for ProductRuntimeHost {}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use tracing::instrument;
    use truapi::versioned::jam_peer_transport::{
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
    use truapi::{CallContext, CallError};

    use super::ProductRuntimeHost;

    #[truapi::async_trait]
    impl truapi::api::JamPeerTransport for ProductRuntimeHost {
        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.dial"))]
        async fn dial(
            &self,
            cx: &CallContext,
            request: HostJamPeerTransportDialRequest,
        ) -> Result<HostJamPeerTransportDialResponse, CallError<HostJamPeerTransportDialError>>
        {
            let HostJamPeerTransportDialRequest::V1(inner) = &request;
            let genesis = inner.genesis;
            self.jam_peers
                .dial(
                    cx,
                    request,
                    || self.require_jam_peers(genesis),
                    &self.services.spawner,
                )
                .await
        }

        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.open"))]
        async fn open(
            &self,
            _cx: &CallContext,
            request: HostJamPeerTransportOpenRequest,
        ) -> Result<HostJamPeerTransportOpenResponse, CallError<HostJamPeerTransportOpenError>>
        {
            self.jam_peers.open(request).await
        }

        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.send"))]
        async fn send(
            &self,
            _cx: &CallContext,
            request: HostJamPeerTransportSendRequest,
        ) -> Result<HostJamPeerTransportSendResponse, CallError<HostJamPeerTransportSendError>>
        {
            self.jam_peers.send(request)
        }

        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.recv"))]
        async fn recv(
            &self,
            _cx: &CallContext,
            request: HostJamPeerTransportRecvRequest,
        ) -> Result<HostJamPeerTransportRecvResponse, CallError<HostJamPeerTransportRecvError>>
        {
            self.jam_peers.recv(request)
        }

        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.reset"))]
        async fn reset(
            &self,
            _cx: &CallContext,
            request: HostJamPeerTransportResetRequest,
        ) -> Result<HostJamPeerTransportResetResponse, CallError<HostJamPeerTransportResetError>>
        {
            self.jam_peers.reset(request)
        }

        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.close"))]
        async fn close(
            &self,
            _cx: &CallContext,
            request: HostJamPeerTransportCloseRequest,
        ) -> Result<HostJamPeerTransportCloseResponse, CallError<HostJamPeerTransportCloseError>>
        {
            self.jam_peers.close(request)
        }

        #[instrument(skip_all, fields(runtime.method = "jam_peer_transport.events"))]
        async fn events(
            &self,
            _cx: &CallContext,
            _request: HostJamPeerTransportEventsRequest,
        ) -> Result<HostJamPeerTransportEventsResponse, CallError<HostJamPeerTransportEventsError>>
        {
            self.jam_peers.events()
        }
    }
}
