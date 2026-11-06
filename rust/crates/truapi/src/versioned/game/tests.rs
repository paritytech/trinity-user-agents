use super::*;
use parity_scale_codec::Encode;

// The domain error's discriminant is part of the wire contract.
#[test]
fn remind_errors_keep_their_discriminants() {
	let past = HostRemindNextGameError::V1(v01::HostRemindNextGameError::StartsInPast);

	assert_eq!(hex::encode(past.encode()), "0000");
}
