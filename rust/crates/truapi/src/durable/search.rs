//! [`PinnedChain`]: the [`PinnedView`] a recovery pass reads a chain through.

use futures::{StreamExt, stream};
use subxt::utils::H256;

use super::ladder::{PinnedView, SearchResult};
use crate::{
	chain::{BlockBackend, ChainHeads, HashAndNumber, Heads},
	chain_runtime::RuntimeFailure,
};

/// Blocks read at once while searching a range.
const SEARCH_CONCURRENCY: usize = 16;

/// One chain's heads, read once, with its block data behind them.
pub struct PinnedChain<'a> {
	genesis: H256,
	heads: Heads,
	blocks: &'a dyn BlockBackend,
}

impl<'a> PinnedChain<'a> {
	/// Reads the heads of the chain with `genesis` and pins them.
	pub async fn pin(
		heads: &dyn ChainHeads,
		blocks: &'a dyn BlockBackend,
		genesis: H256,
	) -> Result<Self, RuntimeFailure> {
		Ok(Self { genesis, heads: heads.heads(genesis).await?, blocks })
	}
}

#[async_trait::async_trait]
impl PinnedView for PinnedChain<'_> {
	fn heads(&self) -> Heads {
		self.heads
	}

	async fn search(&self, from: u64, to: u64, tx_hash: H256) -> SearchResult {
		search_range(self.blocks, self.genesis, from, to, tx_hash).await
	}
}

/// Looks for `tx_hash` in the canonical blocks `from..=to`, reading hashes and
/// bodies concurrently. An unreadable block only ends the unbroken read that
/// absence is proven over: the search goes on, because a hit is evidence
/// whatever was skipped.
async fn search_range(
	blocks: &dyn BlockBackend,
	genesis: H256,
	from: u64,
	to: u64,
	tx_hash: H256,
) -> SearchResult {
	let mut bodies = stream::iter(from..=to)
		.map(|number| read_body(blocks, genesis, number))
		.buffered(SEARCH_CONCURRENCY);

	let mut read_advanced_to = None;
	let mut unbroken = true;
	while let Some(read) = bodies.next().await {
		let Some((block, body)) = read else {
			unbroken = false;
			continue;
		};
		if body.contains(&tx_hash) {
			let outcome = blocks.dispatch_outcome(genesis, block, tx_hash).await.ok().flatten();
			return SearchResult::Found { block, outcome };
		}
		if unbroken {
			read_advanced_to = Some(block.number);
		}
	}
	SearchResult::NotFound { read_advanced_to }
}

/// The canonical block at `number` and the extrinsic hashes in it, or
/// `None` when either cannot be read.
async fn read_body(
	blocks: &dyn BlockBackend,
	genesis: H256,
	number: u64,
) -> Option<(HashAndNumber, Vec<H256>)> {
	let hash = blocks.block_hash(genesis, number).await.ok().flatten()?;
	let body = blocks.extrinsic_hashes(genesis, hash).await.ok().flatten()?;
	Some((HashAndNumber { hash, number }, body))
}

#[cfg(test)]
mod tests;
