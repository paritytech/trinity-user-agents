//! A fixed number of reads at a time.
//!
//! A dotNS read holds an Asset Hub follow while it runs, and a connection has
//! room for four. Reads that take a slot first leave the rest of that room to
//! everything else on the connection, however many of them are asked for at
//! once.

use futures::{
	future::select_all,
	lock::{Mutex, MutexGuard},
};

/// Slots a read takes for as long as it runs. A read asked for while every
/// slot is taken waits for one to free.
pub struct ReadSlots {
	slots: Vec<Mutex<()>>,
}

impl ReadSlots {
	/// `count` reads at a time. At least one.
	pub fn new(count: usize) -> Self {
		Self { slots: (0..count.max(1)).map(|_| Mutex::new(())).collect() }
	}

	/// The first slot that frees, held until the guard is dropped.
	pub async fn take(&self) -> MutexGuard<'_, ()> {
		let (slot, _, _) = select_all(self.slots.iter().map(Mutex::lock)).await;
		slot
	}
}
