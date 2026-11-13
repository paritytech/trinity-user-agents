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
	known: futures::lock::Mutex<Option<Known>>,
}

#[derive(Clone, Copy)]
struct Known {
	stored: StoredRun,
	/// The first read found an earlier run.
	started_earlier: bool,
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
		if let Some(known) = *known {
			return Some(known.stored);
		}
		let recorded = storage
			.read(CoreStorageKey::ChainRun)
			.await
			.inspect_err(|error| warn!(reason = %error.reason, "reading the chain run failed"))
			.ok()?;
		let stored = match recorded {
			None => StoredRun::Unrecorded,
			// A record that does not decode names no run the stack is in.
			Some(bytes) if StackRun::decode(&mut bytes.as_slice()) == Ok(stack.run()) => {
				StoredRun::Current
			},
			Some(_) => StoredRun::Earlier,
		};
		*known = Some(Known { stored, started_earlier: stored == StoredRun::Earlier });
		Some(stored)
	}

	/// Whether the core started with chain data from an earlier run.
	///
	/// Unlike [`Self::stored`], this stays true after [`Self::record`]. A
	/// session the host keeps itself was written in that earlier run, and the
	/// host can hand the same copy over again after the core has moved on.
	pub async fn started_earlier(
		&self,
		storage: &StackStorage<'_, impl CoreStorage + ?Sized>,
		stack: &Stack,
	) -> bool {
		self.stored(storage, stack).await;
		self.known.lock().await.is_some_and(|known| known.started_earlier)
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
			Ok(()) => {
				let started_earlier = known.is_some_and(|known| known.started_earlier);
				*known = Some(Known { stored: StoredRun::Current, started_earlier });
			},
			Err(error) => warn!(reason = %error.reason, "recording the chain run failed"),
		}
	}
}

#[cfg(test)]
mod tests;
