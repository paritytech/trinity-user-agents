//! [`PinnedChain`]: the [`PinnedView`] a recovery pass reads a chain through.

use futures::{StreamExt, stream};
use subxt::utils::H256;

use super::ladder::{PinnedView, SearchResult};
use crate::chain::{BlockBackend, ChainHeads, HashAndNumber, Heads};
use crate::chain_runtime::RuntimeFailure;

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
        Ok(Self {
            genesis,
            heads: heads.heads(genesis).await?,
            blocks,
        })
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
pub async fn search_range(
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
            let outcome = blocks
                .dispatch_outcome(genesis, block, tx_hash)
                .await
                .ok()
                .flatten();
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
    let body = blocks
        .extrinsic_hashes(genesis, hash)
        .await
        .ok()
        .flatten()?;
    Some((HashAndNumber { hash, number }, body))
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::chain::DispatchOutcome;
    use crate::durable::testing::{FakeChain, GENESIS, block, block_hash};

    const TX: H256 = H256([7; 32]);

    fn search(chain: &FakeChain, from: u64, to: u64) -> SearchResult {
        block_on(search_range(chain, GENESIS, from, to, TX))
    }

    /// A reorg can reorder extrinsics, so the outcome is read from the block
    /// the extrinsic was actually found in.
    #[test]
    fn a_hit_reports_its_block_and_the_outcome_read_there() {
        let chain = FakeChain::new(130, 140);
        chain.include(110, TX, DispatchOutcome::Failed);

        assert_eq!(
            search(&chain, 100, 130),
            SearchResult::Found {
                block: block(110),
                outcome: Some(DispatchOutcome::Failed)
            }
        );
    }

    #[test]
    fn absence_over_a_fully_read_range_is_complete() {
        let chain = FakeChain::new(130, 140);

        assert_eq!(
            search(&chain, 100, 130),
            SearchResult::NotFound {
                read_advanced_to: Some(130)
            }
        );
    }

    #[test]
    fn an_unreadable_block_hash_ends_the_proven_absence_before_it() {
        let chain = FakeChain::new(130, 140);
        chain.state().unreadable_heights.insert(120);

        assert_eq!(
            search(&chain, 100, 130),
            SearchResult::NotFound {
                read_advanced_to: Some(119)
            }
        );
    }

    #[test]
    fn an_unreadable_body_ends_the_proven_absence_before_it() {
        let chain = FakeChain::new(130, 140);
        chain.state().unreadable_bodies.insert(block_hash(101));

        assert_eq!(
            search(&chain, 100, 130),
            SearchResult::NotFound {
                read_advanced_to: Some(100)
            }
        );
    }

    /// A hit is positive evidence whatever could not be read before it.
    #[test]
    fn a_hit_past_an_unreadable_block_is_still_found() {
        let chain = FakeChain::new(130, 140);
        chain.state().unreadable_heights.insert(105);
        chain.include(110, TX, DispatchOutcome::Succeeded);

        assert_eq!(
            search(&chain, 100, 130),
            SearchResult::Found {
                block: block(110),
                outcome: Some(DispatchOutcome::Succeeded)
            }
        );
    }

    /// The body is still kept but the events are pruned: inclusion alone is not
    /// success.
    #[test]
    fn a_hit_whose_events_cannot_be_read_has_no_outcome() {
        let chain = FakeChain::new(130, 140);
        chain.include(110, TX, DispatchOutcome::Succeeded);
        chain.state().unreadable_outcomes.insert(block_hash(110));

        assert_eq!(
            search(&chain, 100, 130),
            SearchResult::Found {
                block: block(110),
                outcome: None
            }
        );
    }

    /// The search ends at its first hit rather than reading the whole range.
    #[test]
    fn the_search_stops_reading_bodies_past_a_hit() {
        let chain = FakeChain::new(1000, 1000);
        chain.include(1, TX, DispatchOutcome::Succeeded);

        search(&chain, 0, 1000);

        assert!(chain.state().body_reads < 100);
    }

    /// A reorg between passes can change any block of the range, so nothing
    /// read earlier is reused.
    #[test]
    fn every_search_reads_the_range_again() {
        let chain = FakeChain::new(130, 140);

        search(&chain, 100, 102);
        search(&chain, 100, 102);

        assert_eq!(chain.state().body_reads, 6);
    }
}
