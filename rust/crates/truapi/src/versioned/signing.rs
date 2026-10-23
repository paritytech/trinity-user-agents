//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.
//!
//! v0.2 gives `create_transaction` its own error for a node that could not be
//! asked for the chain's current state, and its request and response keep the
//! v0.1 shape. A v0.1 caller
//! cannot decode the new variant, so it receives `Unknown` with the same
//! reason.
//!
//! `create_transaction_with_legacy_account` stays on v0.1. A pairing host
//! always relays it to the signing host, whose answer carries no such
//! variant, so a v0.2 for it would only stop new products reaching hosts that
//! do not serve one yet.

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
	pub enum HostCreateTransactionRequest {
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
	pub enum HostCreateTransactionWithLegacyAccountRequest { V1 => v01::LegacyAccountTxPayload }
	pub enum HostCreateTransactionWithLegacyAccountResponse { V1 => v01::HostCreateTransactionWithLegacyAccountResponse }
	pub enum HostCreateTransactionWithLegacyAccountError { V1 => v01::HostCreateTransactionError }
}

// The request and response keep their v0.1 shape. They still gain a V2
// variant, because a method's version is uniform across its request, response
// and error.

impl IntoLatest for HostCreateTransactionRequest {
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
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(match latest {
			v02::HostCreateTransactionError::FailedToDecode => {
				v01::HostCreateTransactionError::FailedToDecode
			},
			v02::HostCreateTransactionError::Rejected => v01::HostCreateTransactionError::Rejected,
			v02::HostCreateTransactionError::NotSupported { reason } => {
				v01::HostCreateTransactionError::NotSupported { reason }
			},
			v02::HostCreateTransactionError::PermissionDenied => {
				v01::HostCreateTransactionError::PermissionDenied
			},
			v02::HostCreateTransactionError::Unknown { reason } |
			v02::HostCreateTransactionError::ChainUnavailable { reason } => {
				v01::HostCreateTransactionError::Unknown { reason }
			},
			v02::HostCreateTransactionError::UnknownContact => {
				v01::HostCreateTransactionError::UnknownContact
			},
		})
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
mod tests {
	use super::*;
	use crate::versioned::Versioned;

	fn chain_unavailable() -> v02::HostCreateTransactionError {
		v02::HostCreateTransactionError::ChainUnavailable {
			reason: "cannot select a metadata block".to_string(),
		}
	}

	#[test]
	fn chain_unavailable_reaches_a_v02_caller_as_itself() {
		// The variant exists so a product can retry on it alone.
		assert_eq!(
			HostCreateTransactionError::from_latest(chain_unavailable(), 2),
			HostCreateTransactionError::V2(chain_unavailable())
		);
	}

	#[test]
	fn chain_unavailable_reaches_a_v01_caller_as_unknown() {
		// A v0.1 product cannot decode the new variant, so it gets `Unknown`
		// with the same reason.
		assert_eq!(
			HostCreateTransactionError::from_latest(chain_unavailable(), 1),
			HostCreateTransactionError::V1(v01::HostCreateTransactionError::Unknown {
				reason: "cannot select a metadata block".to_string(),
			})
		);
	}

	#[test]
	fn a_v01_error_upgrades_without_becoming_chain_unavailable() {
		// v0.1 has no such variant, so an `Unknown` it reports stays `Unknown`.
		let reason = "cannot select a metadata block".to_string();
		assert_eq!(
			HostCreateTransactionError::V1(v01::HostCreateTransactionError::Unknown {
				reason: reason.clone()
			})
			.into_latest(),
			v02::HostCreateTransactionError::Unknown { reason }
		);
	}

	#[test]
	fn create_transaction_moves_to_v02_as_a_whole() {
		// A method's request, response and error share one version, and the
		// generated client sends the newest request, so a request left on V1
		// would keep every caller from ever seeing the V2 error.
		assert_eq!(
			[
				HostCreateTransactionRequest::LATEST,
				HostCreateTransactionResponse::LATEST,
				HostCreateTransactionError::LATEST,
			],
			[2; 3]
		);
	}

	#[test]
	fn create_transaction_with_legacy_account_stays_on_v01() {
		// A pairing host relays it, so it can never report the v0.2 variant,
		// and a V2 request would only fail on hosts that do not serve one.
		assert_eq!(HostCreateTransactionWithLegacyAccountRequest::LATEST, 1);
	}

	#[test]
	fn v1_keeps_codec_index_zero_so_the_new_version_is_additive() {
		// Reordering the variants would renumber V1 and break every deployed
		// v0.1 peer.
		use parity_scale_codec::Encode;

		let v1 = HostCreateTransactionError::V1(v01::HostCreateTransactionError::Rejected);
		assert_eq!(v1.encode()[0], 0);
		assert_eq!(v1.version(), 1);
	}
}
