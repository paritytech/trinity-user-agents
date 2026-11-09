use super::*;

const ID: DurableTxId = DurableTxId(1);

/// A policy rebuilt the transaction into the bytes of the attempt that
/// failed. The new attempt gets a watch of its own, and the old watch
/// letting go afterwards must not leave it unowned for recovery to race.
#[test]
fn releasing_an_earlier_claim_leaves_the_current_one_owned() {
	let ownership = Ownership::default();
	let earlier = ownership.acquire(ID);
	ownership.acquire(ID);

	ownership.release(earlier);

	assert!(ownership.is_owned(ID));
}

#[test]
fn a_released_transaction_can_be_owned_again() {
	let ownership = Ownership::default();
	let first = ownership.acquire(ID);
	ownership.release(first);

	ownership.acquire(ID);

	assert!(ownership.is_owned(ID));
}

#[test]
fn releasing_the_current_claim_ends_ownership() {
	let ownership = Ownership::default();
	let claim = ownership.acquire(ID);

	ownership.release(claim);

	assert!(!ownership.is_owned(ID));
}
