//! Watching storage entries: the values of chosen keys, read once per head
//! and reported only when they change.
//!
//! [`WatchedStorage`] reports changes to raw keys. On top of it,
//! `watch_entry_changes` encodes the keys of a subxt storage address, decodes
//! the changed values with the metadata of the runtime they were read under,
//! and starts the watch again whenever it cannot start or ends. A
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

/// How long `watch_entry_changes` waits between attempts to start a watch.
const RETRY_DELAY: Duration = Duration::from_secs(5);

impl dyn WatchedStorage {
	/// Starts one watch of `keys` of the entry `address` names on `genesis`,
	/// at `head`, or returns why it cannot start: the chain is unreachable, or
	/// the runtime has no such entry. The first item has every key, and later
	/// items only the keys whose value changed. A failed read is an `Err`
	/// item, and later heads are still read. Keys are encoded once, with the
	/// metadata current when the watch starts.
	async fn start_entry_watch<A>(
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

	/// Watches `keys` of the entry `address` names on `genesis`, at `head`,
	/// for as long as the stream is held. The first item has every key, and
	/// later items only the keys whose value changed. A failed read is an
	/// `Err` item, and later heads are still read. A watch that cannot start,
	/// because the chain is unreachable, is an `Err` item too, and is started
	/// again five seconds after it failed or its changes ended. A
	/// permanent failure, as [`StorageWatchError::is_permanent`] names it,
	/// such as an entry the runtime does not declare, is the last item.
	pub fn watch_entry_changes<A>(
		self: Arc<Self>,
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
		let timer = self.timer();
		let request = WatchRequest { storage: self, genesis, head, address, keys };
		let first_attempt = request.clone().start_watch_immediately(&timer);
		let retries =
			stream::repeat_with(move || request.clone().start_watch_after(RETRY_DELAY, &timer))
				.flatten();
		stop_after_permanent_failure(first_attempt.chain(retries).boxed())
	}
}

/// What `watch_entry_changes` watches, each time it starts a watch.
struct WatchRequest<A: Address> {
	storage: Arc<dyn WatchedStorage>,
	genesis: H256,
	head: WatchedHead,
	address: A,
	keys: Vec<A::KeyParts>,
}

impl<A> WatchRequest<A>
where
	A: Address + Clone + Send + 'static,
	A::KeyParts: Clone + Send + 'static,
	A::Value: Send + 'static,
{
	/// Starts the watch now; see [`Self::start_watch_after`].
	fn start_watch_immediately(
		self,
		timer: &Arc<dyn Timer>,
	) -> DecodedEntryChangeStream<A::KeyParts, A::Value> {
		self.start_watch_after(Duration::ZERO, timer)
	}

	/// Waits `delay`, starts the watch, and returns its changes until they
	/// end. If the watch cannot start, returns a single `Err` saying why.
	fn start_watch_after(
		self,
		delay: Duration,
		timer: &Arc<dyn Timer>,
	) -> DecodedEntryChangeStream<A::KeyParts, A::Value> {
		let pause = timer.sleep(delay);
		let started_watch = async move {
			pause.await;
			self.start_watch().await
		};
		stream::once(started_watch)
			.flat_map(|started_watch| started_watch.unwrap_or_else(single_error))
			.boxed()
	}

	/// Starts the watch, or returns why it cannot start.
	async fn start_watch(
		self,
	) -> Result<DecodedEntryChangeStream<A::KeyParts, A::Value>, StorageWatchError> {
		self.storage
			.start_entry_watch(self.genesis, self.head, self.address, self.keys)
			.await
	}
}

impl<A: Address + Clone> Clone for WatchRequest<A>
where
	A::KeyParts: Clone,
{
	fn clone(&self) -> Self {
		let (address, keys) = (self.address.clone(), self.keys.clone());
		Self {
			storage: self.storage.clone(),
			genesis: self.genesis,
			head: self.head,
			address,
			keys,
		}
	}
}

/// A stream whose one item is `error`.
fn single_error<T: Send + 'static>(
	error: StorageWatchError,
) -> BoxStream<'static, Result<T, StorageWatchError>> {
	stream::once(ready(Err(error))).boxed()
}

/// `changes` up to and including the first permanent failure, then nothing.
fn stop_after_permanent_failure<T: Send + 'static>(
	changes: BoxStream<'static, Result<T, StorageWatchError>>,
) -> BoxStream<'static, Result<T, StorageWatchError>> {
	take_until_inclusive(changes, |change| {
		change.as_ref().is_err_and(StorageWatchError::is_permanent)
	})
}

/// `items` up to and including the first one `is_last` picks, then nothing.
pub fn take_until_inclusive<T: Send + 'static>(
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
