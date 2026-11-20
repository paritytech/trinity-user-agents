use super::*;

const ID: DurableTxId = DurableTxId(1);

#[test]
fn a_released_transaction_can_be_owned_again() {
	let ownership = Ownership::default();
	ownership.acquire(ID);
	ownership.release(ID);

	ownership.acquire(ID);

	assert!(ownership.is_owned(ID));
}

#[test]
fn releasing_a_transaction_ends_ownership() {
	let ownership = Ownership::default();
	ownership.acquire(ID);

	ownership.release(ID);

	assert!(!ownership.is_owned(ID));
}
