//! [`TxSubmitter`] for `ChainRuntime`.

use futures::stream::{self, BoxStream, StreamExt};
use subxt::SubstrateConfig;
use subxt::client::OnlineClientAtBlockImpl;
use subxt::tx::{TransactionProgress, TransactionStatus};
use subxt::utils::H256;

use super::failure;
use crate::chain::{EncodedExtrinsic, TxSubmitter, WatchEvent};
use crate::chain_runtime::{ChainRuntime, RuntimeFailure};

const SUBMIT_AND_WATCH: &str = "submit_and_watch";

#[async_trait::async_trait]
impl TxSubmitter for ChainRuntime {
    async fn submit_and_watch(
        &self,
        genesis: H256,
        extrinsic: &EncodedExtrinsic,
    ) -> Result<BoxStream<'static, WatchEvent>, RuntimeFailure> {
        let progress = self.submit(genesis, extrinsic).await?;
        Ok(watch_events(progress))
    }
}

impl ChainRuntime {
    /// Send `extrinsic` through the shared chainHead client and start
    /// watching it.
    async fn submit(
        &self,
        genesis: H256,
        extrinsic: &EncodedExtrinsic,
    ) -> Result<
        TransactionProgress<SubstrateConfig, OnlineClientAtBlockImpl<SubstrateConfig>>,
        RuntimeFailure,
    > {
        self.online_client(genesis.as_bytes())
            .await?
            .tx()
            .await
            .map_err(|error| failure(SUBMIT_AND_WATCH, error))?
            .from_bytes(extrinsic.bytes().to_vec())
            .submit_and_watch()
            .await
            .map_err(|error| failure(SUBMIT_AND_WATCH, error))
    }
}

/// The watch events of `progress`, up to and including the terminal one. The
/// watch is dropped with its terminal event, so the subscription does not
/// outlive it.
fn watch_events(
    progress: TransactionProgress<SubstrateConfig, OnlineClientAtBlockImpl<SubstrateConfig>>,
) -> BoxStream<'static, WatchEvent> {
    stream::unfold(Some(progress), |progress| async move {
        let mut progress = progress?;
        loop {
            let event = match progress.next().await? {
                Ok(TransactionStatus::Validated | TransactionStatus::Broadcasted) => continue,
                Ok(TransactionStatus::NoLongerInBestBlock) => WatchEvent::NoLongerInBestBlock,
                Ok(TransactionStatus::InBestBlock(block)) => {
                    WatchEvent::InBestBlock(block.block_hash())
                }
                Ok(TransactionStatus::InFinalizedBlock(block)) => {
                    WatchEvent::InFinalizedBlock(block.block_hash())
                }
                Ok(TransactionStatus::Invalid { message }) => WatchEvent::Invalid(message),
                Ok(TransactionStatus::Dropped { message }) => WatchEvent::Dropped(message),
                Ok(TransactionStatus::Error { message }) => WatchEvent::Error(message),
                Err(error) => WatchEvent::Error(error.to_string()),
            };
            let next = (!event.is_terminal()).then_some(progress);
            return Some((event, next));
        }
    })
    .boxed()
}
