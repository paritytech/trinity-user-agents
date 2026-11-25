use super::*;
use parity_scale_codec::{Decode, Encode};

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

fn upgrade<T: Decode + IntoLatest>(frame: &str) -> T::Latest {
	let bytes = hex::decode(frame).expect("test frames are valid hex");
	T::decode(&mut bytes.as_slice()).expect("test frames decode").into_latest()
}

// A v0.1 product sends `shown`, the opposite of `covered`. Its frames must keep
// their meaning, or a page that hides the card behind a modal would hand the
// card its touches back.
#[test]
fn v1_cover_request_upgrades_with_its_flag_inverted() {
	let (hide_card, show_card) = ("0000", "0001");

	assert_eq!(
		(
			upgrade::<HostExpandedCardSetCardCoveredRequest>(hide_card),
			upgrade::<HostExpandedCardSetCardCoveredRequest>(show_card),
		),
		(
			v02::HostExpandedCardSetCardCoveredRequest { covered: true },
			v02::HostExpandedCardSetCardCoveredRequest { covered: false },
		)
	);
}

#[test]
fn v1_turn_request_upgrades_to_the_side_it_asked_for() {
	let (show_back, show_front) = ("0001", "0000");

	assert_eq!(
		(
			upgrade::<HostExpandedCardTurnCardRequest>(show_back),
			upgrade::<HostExpandedCardTurnCardRequest>(show_front),
		),
		(
			v02::HostExpandedCardTurnCardRequest { side: v02::PocketCardSide::Back },
			v02::HostExpandedCardTurnCardRequest { side: v02::PocketCardSide::Front },
		)
	);
}
