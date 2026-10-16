//! Versioned wrappers for [`System`](crate::api::System) methods.
//!
//! v0.2 adds the embedded TrUAPI runtime version to the host info, so a
//! product can tell which runtime release answers it. The request and error
//! keep their v0.1 shape.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostHandshakeRequest { V1 => v01::HostHandshakeRequest }
	pub enum HostHandshakeResponse { V1 }
	pub enum HostHandshakeError { V1 => v01::HostHandshakeError }
	pub enum HostFeatureSupportedRequest { V1 => v01::HostFeatureSupportedRequest }
	pub enum HostFeatureSupportedResponse { V1 => v01::HostFeatureSupportedResponse }
	pub enum HostFeatureSupportedError { V1 => v01::GenericError }
	pub enum HostNavigateToRequest { V1 => v01::HostNavigateToRequest }
	pub enum HostNavigateToResponse { V1 }
	pub enum HostNavigateToError { V1 => v01::HostNavigateToError }
	pub enum HostInfoRequest { V1, V2 }
	pub enum HostInfoResponse {
		V1 => v01::HostInfo,
		V2 => v02::HostInfo,
	}
	pub enum HostInfoError {
		V1 => v01::GenericError,
		V2 => v01::GenericError,
	}
	pub enum HostGetProductContextRequest { V1 }
	pub enum HostGetProductContextResponse { V1 => v01::HostGetProductContextResponse }
	pub enum HostGetProductContextError { V1 => v01::GenericError }
}

// The request and error gain a V2 variant without changing shape, because a
// method's version is uniform across its request, response and error.

impl IntoLatest for HostInfoRequest {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostInfoRequest {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for HostInfoResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostInfo { platform, name, version }) => {
				v02::HostInfo { platform, name, version, truapi_version: String::new() }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostInfoResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		let v02::HostInfo { platform, name, version, truapi_version: _ } = latest;
		Self::V1(v01::HostInfo { platform, name, version })
	}
}

impl IntoLatest for HostInfoError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) | Self::V2(error) => error,
		}
	}
}

impl FromLatest for HostInfoError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::versioned::Versioned;

	fn latest_info() -> v02::HostInfo {
		v02::HostInfo {
			platform: v01::HostPlatform::Ios,
			name: "Polkadot Mobile".to_string(),
			version: "1.2.3".to_string(),
			truapi_version: "0.25.0".to_string(),
		}
	}

	#[test]
	fn a_v02_peer_receives_the_truapi_version() {
		assert_eq!(
			HostInfoResponse::from_latest(latest_info(), 2),
			HostInfoResponse::V2(latest_info())
		);
	}

	#[test]
	fn a_v01_peer_receives_the_host_info_it_already_decodes() {
		// A deployed v0.1 product decodes exactly three fields, so the
		// downgrade must drop the new one rather than append it.
		assert_eq!(
			HostInfoResponse::from_latest(latest_info(), 1),
			HostInfoResponse::V1(v01::HostInfo {
				platform: v01::HostPlatform::Ios,
				name: "Polkadot Mobile".to_string(),
				version: "1.2.3".to_string(),
			})
		);
	}

	#[test]
	fn a_v01_answer_upgrades_without_inventing_a_truapi_version() {
		let upgraded = HostInfoResponse::V1(v01::HostInfo {
			platform: v01::HostPlatform::Web,
			name: "dotli".to_string(),
			version: "1.0.0".to_string(),
		})
		.into_latest();
		assert_eq!(
			upgraded,
			v02::HostInfo {
				platform: v01::HostPlatform::Web,
				name: "dotli".to_string(),
				version: "1.0.0".to_string(),
				truapi_version: String::new(),
			}
		);
	}

	#[test]
	fn v1_keeps_codec_index_zero_so_the_new_version_is_additive() {
		use parity_scale_codec::Encode;

		assert_eq!(HostInfoRequest::V1.encode(), vec![0]);
		assert_eq!(HostInfoRequest::LATEST, 2);
		assert_eq!(HostInfoResponse::LATEST, 2);
		assert_eq!(HostInfoError::LATEST, 2);
	}
}
