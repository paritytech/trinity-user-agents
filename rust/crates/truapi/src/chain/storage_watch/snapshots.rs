//! [`StorageSnapshots`]: raw storage keys, read once per head.

use futures::stream::BoxStream;
use subxt::{ArcMetadata, utils::H256};

use super::WatchedHead;
use crate::{chain::HashAndNumber, chain_runtime::RuntimeFailure};

/// Values of raw storage keys, as of one block.
#[derive(Clone, Debug)]
pub struct StorageSnapshot {
	/// Block the values were read at.
	pub at: HashAndNumber,
	/// Metadata of the runtime the values were read under.
	pub metadata: ArcMetadata,
	/// The value of each requested key, in request order. `None` is an
	/// absent entry.
	pub values: Vec<Option<Vec<u8>>>,
}

/// Storage of a chain, read once per head.
#[async_trait::async_trait]
pub trait StorageSnapshots: Send + Sync {
	/// Metadata of the runtime at the current finalized head, for encoding
	/// keys before a watch starts.
	async fn current_metadata(&self, genesis: H256) -> Result<ArcMetadata, RuntimeFailure>;

	/// One snapshot of `keys` at the current `head`, then one for each later
	/// head. Heads that arrive during a read are skipped in favour of the
	/// newest. A failed read is an `Err` item, never an absent value, and
	/// later heads are still read.
	async fn snapshots(
		&self,
		genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>, RuntimeFailure>;
}
