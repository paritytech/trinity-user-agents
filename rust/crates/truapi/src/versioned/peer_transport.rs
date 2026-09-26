//! Versioned wrappers for [`PeerTransport`](crate::api::PeerTransport) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostPeerTransportDialRequest { V1 => v01::HostPeerTransportDialRequest }
    pub enum HostPeerTransportDialResponse { V1 => v01::HostPeerTransportDialResponse }
    pub enum HostPeerTransportDialError { V1 => v01::HostPeerTransportDialError }
    pub enum HostPeerTransportOpenRequest { V1 => v01::HostPeerTransportOpenRequest }
    pub enum HostPeerTransportOpenResponse { V1 => v01::HostPeerTransportOpenResponse }
    pub enum HostPeerTransportOpenError { V1 => v01::HostPeerTransportOpenError }
    pub enum HostPeerTransportSendRequest { V1 => v01::HostPeerTransportSendRequest }
    pub enum HostPeerTransportSendResponse { V1 }
    pub enum HostPeerTransportSendError { V1 => v01::HostPeerTransportSendError }
    pub enum HostPeerTransportRecvRequest { V1 => v01::HostPeerTransportRecvRequest }
    pub enum HostPeerTransportRecvResponse { V1 => v01::HostPeerTransportRecvResponse }
    pub enum HostPeerTransportRecvError { V1 => v01::HostPeerTransportRecvError }
    pub enum HostPeerTransportResetRequest { V1 => v01::HostPeerTransportResetRequest }
    pub enum HostPeerTransportResetResponse { V1 }
    pub enum HostPeerTransportResetError { V1 => v01::HostPeerTransportResetError }
    pub enum HostPeerTransportCloseRequest { V1 => v01::HostPeerTransportCloseRequest }
    pub enum HostPeerTransportCloseResponse { V1 }
    pub enum HostPeerTransportCloseError { V1 => v01::HostPeerTransportCloseError }
    pub enum HostPeerTransportEventsRequest { V1 }
    pub enum HostPeerTransportEventsResponse { V1 => v01::HostPeerTransportEventsResponse }
    pub enum HostPeerTransportEventsError { V1 => v01::HostPeerTransportEventsError }
}
