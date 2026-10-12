//! [`TxSubmitter`]: submits extrinsics and reports their progress.

use futures::stream::BoxStream;
use subxt::utils::H256;

use super::EncodedExtrinsic;
use crate::chain_runtime::RuntimeFailure;

/// Progress of a submitted extrinsic. The stream ends after a terminal event,
/// or without one when the watch closes, for example because the connection
/// dropped; treat that like [`WatchEvent::Error`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    /// Included in the block that is currently best.
    InBestBlock(H256),
    /// The best block that included it was retracted.
    NoLongerInBestBlock,
    /// Included in a finalized block. Terminal.
    InFinalizedBlock(H256),
    /// Rejected as invalid. Terminal.
    Invalid(String),
    /// The node stopped watching it. Terminal, but the extrinsic may still
    /// be included.
    Dropped(String),
    /// Watching failed, on the node or in the client, which gives up after
    /// four minutes without finality. Terminal, but the extrinsic may still
    /// be included.
    Error(String),
}

impl WatchEvent {
    /// Whether no further event follows this one.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::InBestBlock(_) | Self::NoLongerInBestBlock)
    }
}

/// Submits extrinsics and reports their progress.
#[async_trait::async_trait]
pub trait TxSubmitter: Send + Sync {
    /// Send `extrinsic` once and watch it. Nothing resubmits it: a dropped or
    /// invalid extrinsic is reported and left to the caller. Neither an `Err`
    /// nor a stream that ends without a terminal event proves the node never
    /// received it.
    async fn submit_and_watch(
        &self,
        genesis: H256,
        extrinsic: &EncodedExtrinsic,
    ) -> Result<BoxStream<'static, WatchEvent>, RuntimeFailure>;
}
