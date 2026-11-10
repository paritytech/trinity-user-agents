//! Versioned wrappers for [`LocalStorage`](crate::api::LocalStorage) methods.
//!
//! v0.2 widens the read request from a bare key to a key plus the product whose
//! storage it addresses, and gives the read its own error so it can carry a
//! refusal. A v0.1 caller has no way to name storage other than its own, so
//! upgrading its request means filling the new field with `None`, which is
//! exactly what v0.1 meant.
//!
//! Write and clear keep their v0.1 shape. The manifest grants a read-only
//! `storage` scope and nothing else, so no caller can ever address another
//! product's storage for a write, and there is nothing for those requests to
//! address.

use alloc::string::ToString;

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostLocalStorageReadRequest {
		V1 => v01::HostLocalStorageReadRequest,
		V2 => v02::HostLocalStorageReadRequest,
	}
	pub enum HostLocalStorageReadResponse {
		V1 => v01::HostLocalStorageReadResponse,
		V2 => v01::HostLocalStorageReadResponse,
	}
	pub enum HostLocalStorageReadError {
		V1 => v01::HostLocalStorageReadError,
		V2 => v02::HostLocalStorageReadError,
	}
	pub enum HostLocalStorageWriteRequest { V1 => v01::HostLocalStorageWriteRequest }
	pub enum HostLocalStorageWriteResponse { V1 }
	pub enum HostLocalStorageWriteError { V1 => v01::HostLocalStorageReadError }
	pub enum HostLocalStorageClearRequest { V1 => v01::HostLocalStorageClearRequest }
	pub enum HostLocalStorageClearResponse { V1 }
	pub enum HostLocalStorageClearError { V1 => v01::HostLocalStorageReadError }
	pub enum HostLocalStorageSubscribeRequest { V1 => v01::HostLocalStorageSubscribeRequest }
	pub enum HostLocalStorageChangeItem { V1 => v01::HostLocalStorageChangeItem }
	pub enum HostLocalStorageSubscribeError { V1 => v01::GenericError }
}

impl IntoLatest for HostLocalStorageReadRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostLocalStorageReadRequest { key }) => {
				v02::HostLocalStorageReadRequest { product: None, key }
			},
			Self::V2(latest) => latest,
		}
	}
}

// The read response did not change shape in v0.2. It still gains a V2 variant,
// because a method's version is uniform across its request, response and error
// — without one the generated client would keep every `local_storage.read` call
// pinned to V1 and no product could reach the new field.

impl IntoLatest for HostLocalStorageReadResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostLocalStorageReadResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostLocalStorageReadError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) => error.into(),
			Self::V2(latest) => latest,
		}
	}
}

impl From<v01::HostLocalStorageReadError> for v02::HostLocalStorageReadError {
	fn from(error: v01::HostLocalStorageReadError) -> Self {
		match error {
			v01::HostLocalStorageReadError::Full => Self::Full,
			v01::HostLocalStorageReadError::Unknown { reason } => Self::Unknown { reason },
		}
	}
}

impl FromLatest for HostLocalStorageReadError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(match latest {
			v02::HostLocalStorageReadError::Full => v01::HostLocalStorageReadError::Full,
			v02::HostLocalStorageReadError::Unknown { reason } => {
				v01::HostLocalStorageReadError::Unknown { reason }
			},
			// A v0.1 caller cannot address foreign storage — its request
			// carries no product — so it never provokes this. The downgrade
			// still has to be total, and the variant's own text says it.
			refusal @ v02::HostLocalStorageReadError::AccessNotGranted => {
				v01::HostLocalStorageReadError::Unknown { reason: refusal.to_string() }
			},
		})
	}
}

#[cfg(test)]
mod tests;
