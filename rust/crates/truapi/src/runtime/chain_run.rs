//! Whether the stack's chains restarted since the core last stored what it
//! read from them.
//!
//! A wipe keeps a stack's id and replaces its chains. The session's
//! usernames, allowance keys and the ring-VRF registry were read from the old
//! chains, and nothing in them says so. [`ChainRun`] compares the stack's
//! [`StackRun`] with the one recorded in [`CoreStorageKey::ChainRun`], so the
//! role that owns a session can drop them and then record the new run.

use parity_scale_codec::{Decode, Encode};
use tracing::warn;

use crate::platform::{CoreStorage, CoreStorageKey, Stack, StackRun, StackStorage};

/// What the recorded run says about the chain data the core stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredRun {
	/// It was read in the run the stack is in.
	Current,
	/// No run is recorded, so there is nothing to tell it apart by.
	Unrecorded,
	/// It was read in an earlier run, from chains that no longer exist.
	Earlier,
}

/// Reads the recorded run once and remembers the answer until
/// [`Self::record`] replaces it.
#[derive(Default)]
pub struct ChainRun {
	known: futures::lock::Mutex<Option<StoredRun>>,
}

impl ChainRun {
	/// What the recorded run says, or `None` when the host could not read
	/// it. A failed read is not remembered, so the next call asks again.
	pub async fn stored(
		&self,
		storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
		stack: &Stack,
	) -> Option<StoredRun> {
		let mut known = self.known.lock().await;
		if known.is_some() {
			return *known;
		}
		let recorded = storage
			.read(CoreStorageKey::ChainRun)
			.await
			.inspect_err(|error| warn!(reason = %error.reason, "reading the chain run failed"))
			.ok()?;
		*known = Some(match recorded {
			None => StoredRun::Unrecorded,
			// A record that does not decode names no run the stack is in.
			Some(bytes) if StackRun::decode(&mut bytes.as_slice()) == Ok(stack.run()) => {
				StoredRun::Current
			},
			Some(_) => StoredRun::Earlier,
		});
		*known
	}

	/// Records that the stored chain data belongs to the run the stack is
	/// in. Call it only after everything from an earlier run is dropped: a
	/// record written first would hide what is left.
	///
	/// A failed write leaves the earlier record, so the data is dropped again
	/// the next time the core starts.
	pub async fn record(
		&self,
		storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
		stack: &Stack,
	) {
		let mut known = self.known.lock().await;
		match storage.write(CoreStorageKey::ChainRun, stack.run().encode()).await {
			Ok(()) => *known = Some(StoredRun::Current),
			Err(error) => warn!(reason = %error.reason, "recording the chain run failed"),
		}
	}
}

#[cfg(test)]
mod tests;
