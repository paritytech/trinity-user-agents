use super::*;
use parity_scale_codec::Encode;

// A Widget kept loaded after its card closed gets this error from either
// call, and its discriminant must stay first so older clients keep decoding it.
#[test]
fn not_presented_error_is_discriminant_zero() {
	let covered = HostExpandedCardSetCardCoveredError::V1(
		v01::HostExpandedCardSetCardCoveredError::NotPresented,
	);
	let turn = HostExpandedCardTurnCardError::V1(v01::HostExpandedCardTurnCardError::NotPresented);

	assert_eq!(
		(hex::encode(covered.encode()), hex::encode(turn.encode())),
		("0000".into(), "0000".into())
	);
}
