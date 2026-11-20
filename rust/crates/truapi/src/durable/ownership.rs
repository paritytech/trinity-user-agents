//! Which transactions a submission watch owns, so a recovery pass skips them
//! and each transaction has one writer at a time.

use std::collections::HashSet;

use futures::channel::mpsc;
use parking_lot::Mutex;

use super::model::DurableTxId;
use crate::wake::WakeSignal;

/// The transactions submission watches currently own.
///
/// Held in memory only: after a crash nothing is owned, which is right,
/// because no watch survived it. A transaction has at most one watch: the
/// executor builds no new attempt for a transaction that is still owned, so
/// only the watch that acquired a transaction releases it.
#[derive(Default)]
pub struct Ownership {
	owned: Mutex<HashSet<DurableTxId>>,
	releases: WakeSignal,
}

impl Ownership {
	/// Takes ownership of `id` for a new attempt.
	pub fn acquire(&self, id: DurableTxId) {
		self.owned.lock().insert(id);
	}

	/// Ends ownership of `id`.
	pub fn release(&self, id: DurableTxId) {
		self.owned.lock().remove(&id);
		self.releases.wake();
	}

	/// A wake after every release from now on.
	pub fn releases(&self) -> mpsc::Receiver<()> {
		self.releases.subscribe()
	}

	/// Whether a watch owns `id`.
	pub fn is_owned(&self, id: DurableTxId) -> bool {
		self.owned.lock().contains(&id)
	}
}

#[cfg(test)]
mod tests;
