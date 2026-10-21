//! [`StorageSnapshots`]: raw storage keys, read once per head.

use futures::stream::BoxStream;
use subxt::{ArcMetadata, utils::H256};

use super::WatchedHead;
use crate::{chain::HashAndNumber, chain_runtime::RuntimeFailure};

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
	pub fn map<U>(self, f: impl FnOnce(T) -> U) -> DecodableAtBlock<U> {
		DecodableAtBlock { at: self.at, metadata: self.metadata, value: f(self.value) }
	}
}

/// Storage of a chain, read once per head.
#[async_trait::async_trait]
pub trait StorageSnapshots: Send + Sync {
	/// Metadata of the runtime at the current finalized head, for encoding
	/// keys before a watch starts.
	async fn current_metadata(&self, genesis: H256) -> Result<ArcMetadata, RuntimeFailure>;

	/// The value of each of `keys`, in request order and `None` when absent,
	/// at the current `head` and then at each later head. Heads that arrive
	/// during a read are skipped in favour of the newest. A failed read is an
	/// `Err` item, never an absent value, and later heads are still read.
	async fn snapshots(
		&self,
		genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<
		BoxStream<'static, Result<DecodableAtBlock<Vec<Option<Vec<u8>>>>, RuntimeFailure>>,
		RuntimeFailure,
	>;
}
