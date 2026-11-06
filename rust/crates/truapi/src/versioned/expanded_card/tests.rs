use super::*;
use parity_scale_codec::Encode;

// A Widget kept loaded after its card closed gets this error from either
// call, and its discriminant must stay first so older clients keep decoding it.
#[test]
fn not_presented_error_is_discriminant_zero() {
	let face =
		HostExpandedCardSetFaceShownError::V1(v01::HostExpandedCardSetFaceShownError::NotPresented);
	let back =
		HostExpandedCardSetBackShownError::V1(v01::HostExpandedCardSetBackShownError::NotPresented);

	assert_eq!(
		(hex::encode(face.encode()), hex::encode(back.encode())),
		("0000".into(), "0000".into())
	);
}
