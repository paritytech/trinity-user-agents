//! Versioned wrappers for [`PersonChat`](crate::api::PersonChat) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostPersonChatGetProfileRequest { V1 => v01::HostPersonChatGetProfileRequest }
	pub enum HostPersonChatGetProfileResponse { V1 => v01::HostPersonChatGetProfileResponse }
	pub enum HostPersonChatSecretsRequest { V1 => v01::HostPersonChatSecretsRequest }
	pub enum HostPersonChatSecretsResponse { V1 => v01::HostPersonChatSecretsResponse }
	pub enum HostPersonChatProveStatementRequest { V1 => v01::HostPersonChatProveStatementRequest }
	pub enum HostPersonChatProveStatementResponse { V1 => v01::HostPersonChatProveStatementResponse }
	pub enum HostPersonChatProveChatRequestRequest { V1 => v01::HostPersonChatProveChatRequestRequest }
	pub enum HostPersonChatProveChatRequestResponse { V1 => v01::HostPersonChatProveChatRequestResponse }
	pub enum HostPersonChatPushTokensRequest { V1 => v01::HostPersonChatPushTokensRequest }
	pub enum HostPersonChatPushTokensResponse { V1 => v01::HostPersonChatPushTokensResponse }
	pub enum HostPersonChatNotifyPeerRequest { V1 => v01::HostPersonChatNotifyPeerRequest }
	pub enum HostPersonChatNotifyPeerResponse { V1 => v01::HostPersonChatNotifyPeerResponse }
	pub enum HostPersonChatSetPushRoutesRequest { V1 => v01::HostPersonChatSetPushRoutesRequest }
	pub enum HostPersonChatSetPushRoutesResponse { V1 }
	pub enum HostPersonChatHopUploadRequest { V1 => v01::HostPersonChatHopUploadRequest }
	pub enum HostPersonChatHopUploadResponse { V1 => v01::HostPersonChatHopUploadResponse }
	pub enum HostPersonChatHopDownloadRequest { V1 => v01::HostPersonChatHopDownloadRequest }
	pub enum HostPersonChatHopDownloadResponse { V1 => v01::HostPersonChatHopDownloadResponse }
	pub enum HostPersonChatHopAckRequest { V1 => v01::HostPersonChatHopAckRequest }
	pub enum HostPersonChatHopAckResponse { V1 }
	pub enum HostPersonChatPrepareDeliveryRequest { V1 => v01::HostPersonChatPrepareDeliveryRequest }
	pub enum HostPersonChatPrepareDeliveryResponse { V1 => v01::HostPersonChatPrepareDeliveryResponse }
	pub enum HostPersonChatProveDeliveryRequest { V1 => v01::HostPersonChatProveDeliveryRequest }
	pub enum HostPersonChatProveDeliveryResponse { V1 => v01::HostPersonChatProveDeliveryResponse }
	pub enum HostPersonChatPublishContactsRequest { V1 => v01::HostPersonChatPublishContactsRequest }
	pub enum HostPersonChatPublishContactsResponse { V1 }
	pub enum HostPersonChatHistoryExportRequest { V1 => v01::HostPersonChatHistoryExportRequest }
	pub enum HostPersonChatHistoryExportResponse { V1 => v01::HostPersonChatHistoryExportResponse }
	pub enum HostPersonChatHistoryImportedRequest { V1 => v01::HostPersonChatHistoryImportedRequest }
	pub enum HostPersonChatHistoryImportedResponse { V1 }
	pub enum HostPersonChatPaymentReceivedRequest { V1 => v01::HostPersonChatPaymentReceivedRequest }
	pub enum HostPersonChatPaymentReceivedResponse { V1 }
	pub enum HostPersonChatError { V1 => v01::HostPersonChatError }
}
