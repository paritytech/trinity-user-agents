use super::*;

// Persisted sessions and the wire both carry the direction, so its order
// is fixed.
#[test]
fn direction_discriminants_are_stable() {
	assert_eq!((FundingDirection::In.encode(), FundingDirection::Out.encode()), (vec![0], vec![1]));
}
