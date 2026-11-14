//! Versioned wrappers for [`Media`](crate::api::Media) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostMediaError { V1 => v01::HostMediaError }
    pub enum HostMediaGetCapabilitiesRequest { V1 }
    pub enum HostMediaGetCapabilitiesResponse { V1 => v01::MediaCapabilities }
    pub enum HostMediaGetCapabilitiesError { V1 => v01::HostMediaError }
    pub enum HostMediaSessionSubscribeRequest { V1 }
    pub enum HostMediaSessionSubscribeItem { V1 => v01::MediaEvent }
    pub enum HostMediaSessionSubscribeError { V1 => v01::HostMediaError }
    pub enum HostMediaCreateSessionRequest { V1 => v01::HostMediaCreateSessionRequest }
    pub enum HostMediaCreateSessionResponse { V1 => v01::HostMediaCreateSessionResponse }
    pub enum HostMediaCreateSessionError { V1 => v01::HostMediaError }
    pub enum HostMediaAddParticipantRequest { V1 => v01::HostMediaAddParticipantRequest }
    pub enum HostMediaAddParticipantResponse { V1 => v01::HostMediaAddParticipantResponse }
    pub enum HostMediaAddParticipantError { V1 => v01::HostMediaError }
    pub enum HostMediaRespondIncomingRequest { V1 => v01::HostMediaRespondIncomingRequest }
    pub enum HostMediaRespondIncomingResponse { V1 => v01::MediaIncomingResponse }
    pub enum HostMediaRespondIncomingError { V1 => v01::HostMediaError }
    pub enum HostMediaRemoveParticipantRequest { V1 => v01::HostMediaRemoveParticipantRequest }
    pub enum HostMediaRemoveParticipantResponse { V1 }
    pub enum HostMediaRemoveParticipantError { V1 => v01::HostMediaError }
    pub enum HostMediaSetLocalTracksRequest { V1 => v01::HostMediaSetLocalTracksRequest }
    pub enum HostMediaSetLocalTracksResponse { V1 => v01::HostMediaSetLocalTracksResponse }
    pub enum HostMediaSetLocalTracksError { V1 => v01::HostMediaError }
    pub enum HostMediaSetSurfacesRequest { V1 => v01::HostMediaSetSurfacesRequest }
    pub enum HostMediaSetSurfacesResponse { V1 => v01::HostMediaSetSurfacesResponse }
    pub enum HostMediaSetSurfacesError { V1 => v01::HostMediaError }
    pub enum HostMediaEndSessionRequest { V1 => v01::HostMediaEndSessionRequest }
    pub enum HostMediaEndSessionResponse { V1 }
    pub enum HostMediaEndSessionError { V1 => v01::HostMediaError }
    pub enum HostMediaGetOperationRequest { V1 => v01::HostMediaGetOperationRequest }
    pub enum HostMediaGetOperationResponse { V1 => v01::MediaOperationSnapshot }
    pub enum HostMediaGetOperationError { V1 => v01::HostMediaError }
    pub enum HostMediaCancelOperationRequest { V1 => v01::HostMediaCancelOperationRequest }
    pub enum HostMediaCancelOperationResponse { V1 => v01::MediaOperationSnapshot }
    pub enum HostMediaCancelOperationError { V1 => v01::HostMediaError }
}
