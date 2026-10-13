//! Versioned wrappers for [`AuthTokens`](crate::api::AuthTokens) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostAuthTokensStoreRequest { V1 => v01::HostAuthTokensStoreRequest }
	pub enum HostAuthTokensStoreResponse { V1 }
	pub enum HostAuthTokensStoreError { V1 => v01::HostAuthTokensStoreError }
	pub enum HostAuthTokensGetRequest { V1 => v01::HostAuthTokensGetRequest }
	pub enum HostAuthTokensGetResponse { V1 => v01::HostAuthTokensGetResponse }
	pub enum HostAuthTokensGetError { V1 => v01::HostAuthTokensLookupError }
	pub enum HostAuthTokensRemoveRequest { V1 => v01::HostAuthTokensRemoveRequest }
	pub enum HostAuthTokensRemoveResponse { V1 }
	pub enum HostAuthTokensRemoveError { V1 => v01::HostAuthTokensLookupError }
}
