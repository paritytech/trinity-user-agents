//! [`ChainHeads`]: the finalized and best heads of a chain.

use futures::stream::BoxStream;
use subxt::utils::H256;

use super::HashAndNumber;
use crate::chain_runtime::RuntimeFailure;

/// The finalized and best blocks, read together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heads {
    /// Latest finalized block.
    pub finalized: HashAndNumber,
    /// Current best block.
    pub best: HashAndNumber,
}

/// A new head reported by the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadEvent {
    /// A block was finalized.
    Finalized(HashAndNumber),
    /// A block became the best block.
    Best(HashAndNumber),
}

/// Finalized and best heads of a chain.
#[async_trait::async_trait]
pub trait ChainHeads: Send + Sync {
    /// The current finalized and best blocks.
    async fn heads(&self, genesis: H256) -> Result<Heads, RuntimeFailure>;

    /// A fresh stream of the finalized and best blocks the node announces,
    /// starting with the current ones. Finalized blocks it skips are not
    /// filled in. An error is reported as an item and later heads still
    /// follow; the stream ends when both of the node's subscriptions end.
    async fn head_events(
        &self,
        genesis: H256,
    ) -> Result<BoxStream<'static, Result<HeadEvent, RuntimeFailure>>, RuntimeFailure>;
}
