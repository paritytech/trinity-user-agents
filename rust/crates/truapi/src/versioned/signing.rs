//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.
//!
//! v0.2 adds `ChainUnavailable` to `create_transaction`'s error. A v0.1
//! caller gets `Unknown` with the same reason. The legacy-account method stays
//! on v0.1, so a product that calls only it still reaches hosts without v0.2.
//! It reports `ChainUnavailable` as `Unknown`.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostSignPayloadRequest { V1 => v01::HostSignPayloadRequest }
	pub enum HostSignPayloadResponse { V1 => v01::HostSignPayloadResponse }
	pub enum HostSignPayloadError { V1 => v01::HostSignPayloadError }
	pub enum HostSignRawRequest { V1 => v01::HostSignRawRequest }
	pub enum HostSignRawResponse { V1 => v01::HostSignPayloadResponse }
	pub enum HostSignRawError { V1 => v01::HostSignPayloadError }
	pub enum HostSignRawWithLegacyAccountRequest { V1 => v01::HostSignRawWithLegacyAccountRequest }
	pub enum HostSignRawWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
	pub enum HostSignRawWithLegacyAccountError { V1 => v01::HostSignPayloadError }
	pub enum HostSignPayloadWithLegacyAccountRequest { V1 => v01::HostSignPayloadWithLegacyAccountRequest }
	pub enum HostSignPayloadWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
	pub enum HostSignPayloadWithLegacyAccountError { V1 => v01::HostSignPayloadError }
	pub enum ProductAccountTxPayload {
		V1 => v01::ProductAccountTxPayload,
		V2 => v01::ProductAccountTxPayload,
	}
	pub enum HostCreateTransactionResponse {
		V1 => v01::HostCreateTransactionResponse,
		V2 => v01::HostCreateTransactionResponse,
	}
	pub enum HostCreateTransactionError {
		V1 => v01::HostCreateTransactionError,
		V2 => v02::HostCreateTransactionError,
	}
	pub enum LegacyAccountTxPayload { V1 => v01::LegacyAccountTxPayload }
	pub enum HostCreateTransactionWithLegacyAccountResponse { V1 => v01::HostCreateTransactionWithLegacyAccountResponse }
	pub enum HostCreateTransactionWithLegacyAccountError { V1 => v01::HostCreateTransactionError }
}

// The request and response keep their v0.1 shape. They still gain a V2
// variant, because a method's version is uniform across its request, response
// and error.

impl IntoLatest for ProductAccountTxPayload {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl IntoLatest for HostCreateTransactionResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostCreateTransactionResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostCreateTransactionError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) => error.into(),
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostCreateTransactionError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest.into_v01()) }
	}
}

impl v02::HostCreateTransactionError {
	/// This error as a v0.1 caller can decode it: `ChainUnavailable`, which
	/// v0.1 lacks, becomes `Unknown` with the same reason.
	pub fn into_v01(self) -> v01::HostCreateTransactionError {
		match self {
			Self::FailedToDecode => v01::HostCreateTransactionError::FailedToDecode,
			Self::Rejected => v01::HostCreateTransactionError::Rejected,
			Self::NotSupported { reason } => {
				v01::HostCreateTransactionError::NotSupported { reason }
			},
			Self::PermissionDenied => v01::HostCreateTransactionError::PermissionDenied,
			Self::Unknown { reason } | Self::ChainUnavailable { reason } => {
				v01::HostCreateTransactionError::Unknown { reason }
			},
			Self::UnknownContact => v01::HostCreateTransactionError::UnknownContact,
		}
	}
}

impl From<v01::HostCreateTransactionError> for v02::HostCreateTransactionError {
	fn from(error: v01::HostCreateTransactionError) -> Self {
		match error {
			v01::HostCreateTransactionError::FailedToDecode => Self::FailedToDecode,
			v01::HostCreateTransactionError::Rejected => Self::Rejected,
			v01::HostCreateTransactionError::NotSupported { reason } => {
				Self::NotSupported { reason }
			},
			v01::HostCreateTransactionError::PermissionDenied => Self::PermissionDenied,
			v01::HostCreateTransactionError::Unknown { reason } => Self::Unknown { reason },
			v01::HostCreateTransactionError::UnknownContact => Self::UnknownContact,
		}
	}
}

#[cfg(test)]
mod tests;
