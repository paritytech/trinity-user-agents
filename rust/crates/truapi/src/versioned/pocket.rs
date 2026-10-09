//! Versioned wrappers for [`Pocket`](crate::api::Pocket) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostPocketListSubscribeRequest { V1 }
	pub enum HostPocketListSubscribeItem { V1 => v01::HostPocketListSubscribeItem }
	pub enum HostPocketListSubscribeError { V1 => v01::GenericError }
	pub enum HostPocketRemoveCardRequest { V1 => v01::HostPocketRemoveCardRequest }
	pub enum HostPocketRemoveCardResponse { V1 }
	pub enum HostPocketRemoveCardError { V1 => v01::HostPocketRemoveCardError }
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Encode;

	// Privileged cards are the only removal the protocol refuses, and its
	// discriminant must stay first so older clients keep decoding it.
	#[test]
	fn privileged_removal_error_is_discriminant_zero() {
		let error = HostPocketRemoveCardError::V1(v01::HostPocketRemoveCardError::Privileged);

		assert_eq!(hex::encode(error.encode()), "0000");
	}
}
