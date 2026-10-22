//! Watching storage entries: the values of chosen keys, read once per head
//! and reported only when they change.
//!
//! [`WatchedStorage`] reports changes to raw keys. On top of it,
//! `watch_entry_changes` encodes the keys of a subxt storage address and decodes the
//! changed values, with the metadata of the runtime they were read under. A
//! source that reads every key at each head turns its snapshots into changes
//! with [`diffed`]. Such a source may skip heads that arrive during
//! a read, because the next read includes their effect, so a value that
//! changes and changes back meanwhile is never reported. A consumer that needs
//! every transition reads events instead.

use core::{future::ready, time::Duration};
use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};
use subxt::{Metadata, storage::Address, utils::H256};

use super::HashAndNumber;
use crate::{chain_runtime::RuntimeFailure, time::Timer};

mod codec;
mod diff;
mod watched_storage;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

pub use codec::StorageEntryName;
pub use diff::{StorageSnapshot, diffed, value_hash};
pub use watched_storage::{DecodableAtBlock, RawStorageChanges, WatchedStorage};

/// Which heads a watch reads at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchedHead {
	/// Each new best block. After a reorg the new fork is read, and changes
	/// are relative to what was reported last, so a value can change back.
	Best,
	/// Each newly finalized block.
	Finalized,
}

/// Values of watched entries that changed, as of one block.
#[derive(Debug, PartialEq, Eq)]
pub struct DecodedEntryChanges<K, V> {
	/// Block the values were read at.
	pub at: HashAndNumber,
	/// Each watched key whose value changed, with its new value. `None` means
	/// the entry is absent and the storage item has no default.
	pub changed: Vec<(K, Option<V>)>,
}

/// Why a watch could not start, or why one read failed.
#[derive(Debug, thiserror::Error)]
pub enum StorageWatchError {
	/// The chain could not be reached or read.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
	/// The runtime metadata has no such entry.
	#[error("{entry} does not match the runtime metadata: {reason}")]
	Incompatible {
		/// The watched entry.
		entry: StorageEntryName,
		/// What differs.
		reason: String,
	},
	/// A watched key did not encode against the entry's key types.
	#[error("key {index} of {entry} did not encode: {reason}")]
	KeyEncoding {
		/// The watched entry.
		entry: StorageEntryName,
		/// Position of the key among the watched keys.
		index: usize,
		/// Why encoding failed.
		reason: String,
	},
	/// A value did not decode as the address's value type.
	#[error("{entry} at block {} ({:?}) did not decode: {reason}", at.number, at.hash)]
	Decode {
		/// The watched entry.
		entry: StorageEntryName,
		/// Block the value was read at.
		at: HashAndNumber,
		/// Why decoding failed.
		reason: String,
	},
}

impl StorageWatchError {
	/// Whether the same watch would fail the same way again: the runtime
	/// does not declare the entry, or a key does not fit it.
	pub fn is_permanent(&self) -> bool {
		matches!(self, Self::Incompatible { .. } | Self::KeyEncoding { .. })
	}
}

/// Changes to a watched entry, item by item.
type DecodedEntryChangeStream<K, V> =
	BoxStream<'static, Result<DecodedEntryChanges<K, V>, StorageWatchError>>;

/// How long `keep_watching_entry` waits before opening a watch again after
/// opening it failed.
const REOPEN_DELAY: Duration = Duration::from_secs(5);

impl dyn WatchedStorage {
	/// Watches `keys` of the entry `address` names on `genesis`, at `head`.
	/// The first item has every key, and later items only the keys whose
	/// value changed. A failed read is an `Err` item, and later heads are
	/// still read. Keys are encoded once, with the metadata current when the
	/// watch starts.
	pub async fn watch_entry_changes<A>(
		self: Arc<Self>,
		genesis: H256,
		head: WatchedHead,
		address: A,
		keys: Vec<A::KeyParts>,
	) -> Result<DecodedEntryChangeStream<A::KeyParts, A::Value>, StorageWatchError>
	where
		A: Address + Send + 'static,
		A::KeyParts: Clone + Send + 'static,
		A::Value: Send + 'static,
	{
		let client = self.client_at_finalized_head(genesis).await?;
		let encoded = codec::encode_storage_keys(&address, &keys, &client)?;
		let changes = self.watch_raw_key_changes(genesis, head, encoded).await?;
		let decoder = EntryChangesDecoder { address, keys };
		Ok(changes.map(move |raw| decoder.decode_raw_changes(raw?)).boxed())
	}

	/// `watch_entry_changes` for as long as the stream is held. A watch that
	/// cannot be opened is an `Err` item, and is opened again after five
	/// seconds on `timer`. A permanent failure, as
	/// [`StorageWatchError::is_permanent`] names it, is the last item.
	pub fn keep_watching_entry<A>(
		self: Arc<Self>,
		timer: Arc<dyn Timer>,
		genesis: H256,
		head: WatchedHead,
		address: A,
		keys: Vec<A::KeyParts>,
	) -> DecodedEntryChangeStream<A::KeyParts, A::Value>
	where
		A: Address + Clone + Send + 'static,
		A::KeyParts: Clone + Send + 'static,
		A::Value: Send + 'static,
	{
		let open =
			move || self.clone().watch_entry_changes(genesis, head, address.clone(), keys.clone());
		let attempts = stream::unfold(false, move |reopening| {
			let (opening, timer) = (open(), timer.clone());
			async move {
				if reopening {
					timer.sleep(REOPEN_DELAY).await;
				}
				Some((opening.await, true))
			}
		});
		let changes = attempts.flat_map(|opened| opened.unwrap_or_else(failed_to_open));
		end_after(changes.boxed(), |change| {
			change.as_ref().is_err_and(StorageWatchError::is_permanent)
		})
	}
}

/// A watch that could not be opened, as its one item.
fn failed_to_open<T: Send + 'static>(
	error: StorageWatchError,
) -> BoxStream<'static, Result<T, StorageWatchError>> {
	stream::once(ready(Err(error))).boxed()
}

/// `items` up to and including the first that `is_last` picks.
pub fn end_after<T: Send + 'static>(
	items: BoxStream<'static, T>,
	is_last: impl Fn(&T) -> bool + Send + 'static,
) -> BoxStream<'static, T> {
	stream::unfold(Some((items, is_last)), |state| async move {
		let (mut items, is_last) = state?;
		let item = items.next().await?;
		let rest = (!is_last(&item)).then_some((items, is_last));
		Some((item, rest))
	})
	.boxed()
}

/// Turns raw changes of one entry back into the caller's keys and values.
struct EntryChangesDecoder<A: Address> {
	address: A,
	keys: Vec<A::KeyParts>,
}

impl<A> EntryChangesDecoder<A>
where
	A: Address,
	A::KeyParts: Clone,
{
	fn decode_raw_changes(
		&self,
		raw: RawStorageChanges,
	) -> Result<DecodedEntryChanges<A::KeyParts, A::Value>, StorageWatchError> {
		let DecodableAtBlock { at, metadata, value: changed } = raw;
		let changed = changed
			.into_iter()
			.map(|(index, bytes)| self.decode_keyed_value(index, bytes, &metadata, at))
			.collect::<Result<_, _>>()?;
		Ok(DecodedEntryChanges { at, changed })
	}

	fn decode_keyed_value(
		&self,
		index: usize,
		bytes: Option<Vec<u8>>,
		metadata: &Metadata,
		at: HashAndNumber,
	) -> Result<(A::KeyParts, Option<A::Value>), StorageWatchError> {
		let value = codec::decode_value_or_default(&self.address, bytes.as_deref(), metadata, at)?;
		Ok((self.keys[index].clone(), value))
	}
}
