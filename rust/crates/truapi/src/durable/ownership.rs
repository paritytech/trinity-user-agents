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
#[derive(Clone, Copy)]
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
mod tests;
