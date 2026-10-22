//! [`WatchedStorage`]: changes to raw storage keys, as of one head after
//! another.

use futures::stream::BoxStream;
use subxt::{ArcMetadata, client::OnlineClientAtBlock, config::SubstrateConfig, utils::H256};

use super::{StorageSnapshot, WatchedHead};
use std::sync::Arc;

use crate::{chain::HashAndNumber, chain_runtime::RuntimeFailure, time::Timer};

/// A value read at one block, with the metadata of the runtime it was read
/// under, which decodes it.
#[derive(Clone, Debug)]
pub struct DecodableAtBlock<T> {
	/// Block the value was read at.
	pub at: HashAndNumber,
	/// Metadata of the runtime the value was read under.
	pub metadata: ArcMetadata,
	/// What was read.
	pub value: T,
}

impl<T> DecodableAtBlock<T> {
	/// `f` of the value, at the same block and under the same metadata.
	pub fn map_value<U>(self, f: impl FnOnce(T) -> U) -> DecodableAtBlock<U> {
		DecodableAtBlock { at: self.at, metadata: self.metadata, value: f(self.value) }
	}
}

/// The position of each changed key among the watched keys, and its new
/// value, `None` when absent.
pub type RawStorageChanges = DecodableAtBlock<Vec<(usize, Option<Vec<u8>>)>>;

/// Storage of a chain by raw key: watched head after head, or read once at a
/// block.
#[async_trait::async_trait]
pub trait WatchedStorage: Send + Sync {
	/// A client at the current finalized head, used to encode the watched
	/// keys before the watch starts.
	async fn client_at_finalized_head(
		&self,
		genesis: H256,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure>;

	/// Changes to `keys` at `head`. The first item holds every key; each later
	/// one holds the keys whose value changed since the item before. A failed
	/// read is an `Err` item, never an absent value, and the watch goes on.
	async fn watch_raw_key_changes(
		&self,
		genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure>;

	/// Every one of `keys` at the block `at`, read in one request. A key the
	/// node leaves out is an error, never an absent value.
	async fn read_raw_keys_at(
		&self,
		genesis: H256,
		at: H256,
		keys: Vec<Vec<u8>>,
	) -> Result<StorageSnapshot, RuntimeFailure>;

	/// The clock a watch waits on between attempts to start.
	fn timer(&self) -> Arc<dyn Timer>;
}
