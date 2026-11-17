use super::*;
use parity_scale_codec::Encode;

// Privileged cards are the only removal the protocol refuses, and its
// discriminant must stay first so older clients keep decoding it.
#[test]
fn privileged_removal_error_is_discriminant_zero() {
	let error = HostPocketRemoveCardError::V1(v01::HostPocketRemoveCardError::Privileged);

	assert_eq!(hex::encode(error.encode()), "0000");
}
