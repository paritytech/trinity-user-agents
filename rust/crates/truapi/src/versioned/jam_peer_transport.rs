//! Versioned wrappers for [`JamPeerTransport`](crate::api::JamPeerTransport) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostJamPeerTransportDialRequest { V1 => v01::HostJamPeerTransportDialRequest }
    pub enum HostJamPeerTransportDialResponse { V1 => v01::HostJamPeerTransportDialResponse }
    pub enum HostJamPeerTransportDialError { V1 => v01::HostJamPeerTransportDialError }
    pub enum HostJamPeerTransportOpenRequest { V1 => v01::HostJamPeerTransportOpenRequest }
    pub enum HostJamPeerTransportOpenResponse { V1 => v01::HostJamPeerTransportOpenResponse }
    pub enum HostJamPeerTransportOpenError { V1 => v01::HostJamPeerTransportOpenError }
    pub enum HostJamPeerTransportSendRequest { V1 => v01::HostJamPeerTransportSendRequest }
    pub enum HostJamPeerTransportSendResponse { V1 }
    pub enum HostJamPeerTransportSendError { V1 => v01::HostJamPeerTransportSendError }
    pub enum HostJamPeerTransportRecvRequest { V1 => v01::HostJamPeerTransportRecvRequest }
    pub enum HostJamPeerTransportRecvResponse { V1 => v01::HostJamPeerTransportRecvResponse }
    pub enum HostJamPeerTransportRecvError { V1 => v01::HostJamPeerTransportRecvError }
    pub enum HostJamPeerTransportResetRequest { V1 => v01::HostJamPeerTransportResetRequest }
    pub enum HostJamPeerTransportResetResponse { V1 }
    pub enum HostJamPeerTransportResetError { V1 => v01::HostJamPeerTransportResetError }
    pub enum HostJamPeerTransportCloseRequest { V1 => v01::HostJamPeerTransportCloseRequest }
    pub enum HostJamPeerTransportCloseResponse { V1 }
    pub enum HostJamPeerTransportCloseError { V1 => v01::HostJamPeerTransportCloseError }
    pub enum HostJamPeerTransportEventsRequest { V1 }
    pub enum HostJamPeerTransportEventsResponse { V1 => v01::HostJamPeerTransportEventsResponse }
    pub enum HostJamPeerTransportEventsError { V1 => v01::HostJamPeerTransportEventsError }
}
