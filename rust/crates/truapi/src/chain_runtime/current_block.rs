//! Reading the current finalized block, for building transactions.

use core::time::Duration;

use derive_more::Display;
use futures::{FutureExt, TryFutureExt, pin_mut};
use subxt::{
	client::OnlineClientAtBlock, config::SubstrateConfig, error::OnlineClientAtBlockError,
};
use tracing::instrument;
use truapi::CancellationToken;

use super::{
	ChainRuntime, RuntimeFailure, RuntimeFailureKind, chain_halt::retry_while_halted,
	follow_retry::FollowEnd,
};

/// Deadline for reading the current block, connecting included. It is longer
/// than all the halt pauses together, so a late re-follow can still serve.
const CURRENT_BLOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// One failed attempt at reading the current block.
#[derive(Display)]
#[display("{error}")]
struct FailedRead {
	error: CurrentBlockError,
	/// Whether another attempt can succeed: a halt answered one of the
	/// attempt's requests, or the attempt lost its connection to the node.
	retryable: bool,
}

/// Why [`ChainRuntime::current_block`] could not read the chain.
#[derive(Debug, PartialEq, Eq, Display)]
pub enum CurrentBlockError {
	/// The host refused to open a connection, for instance to a chain it does
	/// not serve.
	#[display("{_0}")]
	NoChainConnection(RuntimeFailure),
	/// The chain stayed halted, its node could not be reached, the follow
	/// dropped, or the node did not answer in time. A later call can succeed.
	#[display("{_0}")]
	ChainUnavailable(String),
	/// A failure a retry cannot fix, such as metadata this host cannot use.
	#[display("{_0}")]
	Other(String),
}

impl ChainRuntime {
	/// Subxt client at the current finalized block of `genesis_hash`. Core
	/// scope only, like [`Self::online_client`].
	///
	/// Retries through a halt, and through a lost connection: a node that
	/// could not be reached, a connection that closed under the read, or a
	/// follow that dropped. The deadline is there because Subxt never
	/// abandons a runtime call whose follow stopped. Missing it drops only
	/// this read, so the follow keeps serving the core's other users.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.current_block"))]
	pub async fn current_block(
		&self,
		genesis_hash: &[u8],
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, CurrentBlockError> {
		let never_cancelled = CancellationToken::default();
		// A node that could not be reached may be reached later. A provider
		// that refused will refuse again.
		let no_connection = |failure: RuntimeFailure| {
			if failure.kind() == RuntimeFailureKind::Unavailable {
				FailedRead {
					retryable: true,
					error: CurrentBlockError::ChainUnavailable(failure.to_string()),
				}
			} else {
				FailedRead {
					retryable: false,
					error: CurrentBlockError::NoChainConnection(failure),
				}
			}
		};
		let read_block = || async move {
			let subxt = self.subxt_connection(genesis_hash).await.map_err(no_connection)?;
			let halts = subxt.follow.halts();
			subxt.client.at_current_block().await.map_err(|error| {
				let halted = subxt.follow.halts() > halts;
				// Subxt reports some steps, such as the metadata download, only
				// as text, so a closed connection counts as a dropped follow.
				let follow_end = subxt
					.follow
					.end()
					.or_else(|| subxt.connection_closed().then_some(FollowEnd::Dropped));
				FailedRead {
					retryable: halted || follow_end == Some(FollowEnd::Dropped),
					error: classify(follow_end, halted, &error),
				}
			})
		};
		let read = retry_while_halted(
			read_block,
			|failed: &FailedRead| failed.retryable,
			&*self.timer,
			&never_cancelled,
		)
		.map_err(|failed| failed.error)
		.fuse();
		let deadline = self.timer.sleep(CURRENT_BLOCK_TIMEOUT).fuse();
		pin_mut!(read, deadline);
		futures::select! {
			result = read => result,
			() = deadline => Err(CurrentBlockError::ChainUnavailable(format!(
				"the chain gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
			))),
		}
	}
}

/// Whether a later call can get past `error`. A failure to reach the node is
/// retryable if the read met a halt or the follow ended for a passing reason.
/// Subxt reports every follow end the same way, so `follow_end` decides.
fn classify(
	follow_end: Option<FollowEnd>,
	halted: bool,
	error: &OnlineClientAtBlockError,
) -> CurrentBlockError {
	let retryable = match error {
		// A follow that moved on no longer holds the block it reported.
		OnlineClientAtBlockError::BlockHeaderNotFound { .. } => true,
		OnlineClientAtBlockError::CannotGetCurrentBlock { .. } |
		OnlineClientAtBlockError::CannotGetBlockHash { .. } |
		OnlineClientAtBlockError::CannotGetBlockHeader { .. } |
		OnlineClientAtBlockError::CannotGetSpecVersion { .. } |
		OnlineClientAtBlockError::CannotGetMetadata { .. } => {
			halted || matches!(follow_end, Some(FollowEnd::Halted | FollowEnd::Dropped))
		},
		_ => false,
	};
	let reason = error.to_string();
	if retryable {
		CurrentBlockError::ChainUnavailable(reason)
	} else {
		CurrentBlockError::Other(reason)
	}
}

#[cfg(test)]
mod tests;
