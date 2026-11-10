use super::*;
use parity_scale_codec::Encode;

// A Widget kept loaded after its card closed gets this error, and its
// discriminant must stay first so older clients keep decoding it.
#[test]
fn not_presented_error_is_discriminant_zero() {
	let error =
		HostExpandedCardSetFaceShownError::V1(v01::HostExpandedCardSetFaceShownError::NotPresented);

	assert_eq!(hex::encode(error.encode()), "0000");
}
