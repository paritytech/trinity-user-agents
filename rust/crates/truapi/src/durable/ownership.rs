//! Which transactions a submission watch owns, so a recovery pass skips them
//! and each transaction has one writer at a time.

use std::collections::HashMap;

use parking_lot::Mutex;

use super::model::DurableTxId;

/// The attempts submission watches currently own.
///
/// Held in memory only: after a crash nothing is owned, which is right,
/// because no watch survived it. Each acquisition is its own
/// [`OwnershipClaim`], and only that claim releases it, so releasing an
/// earlier attempt never ends a later one, even one with the same bytes.
#[derive(Default)]
pub struct Ownership {
	state: Mutex<OwnershipState>,
}

#[derive(Default)]
struct OwnershipState {
	owned: HashMap<DurableTxId, u64>,
	next_serial: u64,
}

/// One acquisition of a transaction by a submission watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnershipClaim {
	id: DurableTxId,
	serial: u64,
}

impl OwnershipClaim {
	/// The transaction claimed.
	pub fn id(&self) -> DurableTxId {
		self.id
	}
}

impl Ownership {
	/// Takes ownership of `id` for a new attempt, replacing any earlier
	/// claim on it.
	pub fn acquire(&self, id: DurableTxId) -> OwnershipClaim {
		let mut state = self.state.lock();
		let serial = state.next_serial;
		state.next_serial += 1;
		state.owned.insert(id, serial);
		OwnershipClaim { id, serial }
	}

	/// Ends `claim`, unless a later claim on its transaction replaced it.
	pub fn release(&self, claim: OwnershipClaim) {
		let mut state = self.state.lock();
		if state.owned.get(&claim.id) == Some(&claim.serial) {
			state.owned.remove(&claim.id);
		}
	}

	/// Whether a watch owns `id`.
	pub fn is_owned(&self, id: DurableTxId) -> bool {
		self.state.lock().owned.contains_key(&id)
	}
}

#[cfg(test)]
mod tests {
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
}
