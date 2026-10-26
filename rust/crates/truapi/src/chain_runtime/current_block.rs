//! The chain's current finalized block, for building transactions against its
//! runtime metadata.

use core::time::Duration;

use derive_more::Display;
use futures::{FutureExt, pin_mut};
use subxt::{
	client::OnlineClientAtBlock, config::SubstrateConfig, error::OnlineClientAtBlockError,
};

use super::{ChainRuntime, ConnectionScope, RuntimeFailure, follow_retry::FollowEnd};

/// How long reading the current block may take: longer than the pauses
/// between refused follows add up to, so a follow accepted at the last
/// attempt still serves the read.
const CURRENT_BLOCK_TIMEOUT: Duration = Duration::from_secs(45);

/// Why [`ChainRuntime::current_block`] could not read the chain.
#[derive(Debug, PartialEq, Eq, Display)]
pub enum CurrentBlockError {
	/// No connection to the chain could be opened, for instance because the
	/// host serves no such chain.
	#[display("{_0}")]
	NoConnection(RuntimeFailure),
	/// The node could not be asked: its follow was refused until the retries
	/// ran out or was dropped, or it gave no answer in time. A later call can
	/// succeed.
	#[display("{_0}")]
	Unavailable(String),
	/// The node answered with nothing this host can use, or its provider
	/// cannot follow the chain at all.
	#[display("{_0}")]
	Failed(String),
}

impl ChainRuntime {
	/// Subxt client at the current finalized block of `genesis_hash`.
	///
	/// Core scope only, for the same reason as [`Self::online_client`].
	///
	/// The read has a deadline because a runtime call Subxt started on a
	/// follow that then stopped waits for an answer the next follow never
	/// sends, and nothing else ends that wait.
	pub async fn current_block(
		&self,
		genesis_hash: &[u8],
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, CurrentBlockError> {
		debug_assert!(
			matches!(self.scope, ConnectionScope::Core),
			"the Subxt backend's driver task defeats release_scope on a product scope",
		);
		let connection = self
			.subxt_connection(genesis_hash)
			.await
			.map_err(CurrentBlockError::NoConnection)?;
		let read = connection.client.at_current_block().fuse();
		let deadline = self.timer.sleep(CURRENT_BLOCK_TIMEOUT).fuse();
		pin_mut!(read, deadline);
		futures::select! {
			result = read => result.map_err(|error| classify(connection.follow_end(), &error)),
			() = deadline => Err(CurrentBlockError::Unavailable(format!(
				"the node gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
			))),
		}
	}
}

/// Sort a failure to select the current block by whether a later call can get
/// past it. Subxt reports every follow end as a dropped subscription, so the
/// recorded cause decides.
fn classify(follow_end: Option<FollowEnd>, error: &OnlineClientAtBlockError) -> CurrentBlockError {
	let reason = error.to_string();
	match follow_end {
		Some(FollowEnd::Unservable) => CurrentBlockError::Failed(reason),
		Some(FollowEnd::Refused | FollowEnd::Dropped) => CurrentBlockError::Unavailable(reason),
		// A follow that moved on no longer holds the block it reported.
		None if matches!(error, OnlineClientAtBlockError::BlockHeaderNotFound { .. }) => {
			CurrentBlockError::Unavailable(reason)
		},
		None => CurrentBlockError::Failed(reason),
	}
}

#[cfg(test)]
mod tests {
	use std::{sync::Arc, thread};

	use async_trait::async_trait;
	use futures::executor::block_on;
	use subxt::error::{BackendError, Hex, RpcError as SubxtRpcError};

	use super::*;
	use crate::{
		chain_runtime::{RuntimeChainProvider, follow_retry::REFUSED_FOLLOW_PAUSES},
		platform::JsonRpcConnection,
		test_support::{ScriptedChainHead, test_spawner, wait_for_sent, wait_until},
		time::ManualTimer,
	};

	const GENESIS: [u8; 32] = [0xbb; 32];

	/// Read the current block on its own thread over `node`, with a clock that
	/// moves only when the test advances it.
	fn read_current_block(
		node: ScriptedChainHead,
	) -> (thread::JoinHandle<Result<(), CurrentBlockError>>, Arc<ManualTimer>) {
		let timer = Arc::new(ManualTimer::default());
		let (_provider, chain) = node.serve();
		let chain = chain.with_timer(timer.clone());
		let read = thread::spawn(move || block_on(chain.current_block(&GENESIS)).map(drop));
		(read, timer)
	}

	/// Subxt reports every follow end alike, so the recorded cause alone
	/// tells a halted chain, worth asking again, from a provider that can
	/// never follow it, and a halt is retryable whichever read it interrupts.
	#[test]
	fn a_failure_is_retryable_only_when_a_later_call_can_get_past_it() {
		let dropped = || OnlineClientAtBlockError::CannotGetCurrentBlock {
			reason: BackendError::Rpc(SubxtRpcError::SubscriptionDropped),
		};
		let unavailable =
			|error: &OnlineClientAtBlockError| CurrentBlockError::Unavailable(error.to_string());
		let failed =
			|error: &OnlineClientAtBlockError| CurrentBlockError::Failed(error.to_string());
		let metadata_fetch = OnlineClientAtBlockError::CannotGetMetadata {
			block_hash: Hex::from([1; 32]),
			reason: "subscription dropped".to_string(),
		};
		let stale_block =
			OnlineClientAtBlockError::BlockHeaderNotFound { block_hash: Hex::from([1; 32]) };
		let unusable = OnlineClientAtBlockError::UnsupportedMetadataVersion {
			block_hash: Hex::from([1; 32]),
			version: 13,
		};

		let cases = [
			(Some(FollowEnd::Refused), dropped()),
			(Some(FollowEnd::Dropped), dropped()),
			(Some(FollowEnd::Unservable), dropped()),
			(Some(FollowEnd::Refused), metadata_fetch),
			(None, stale_block),
			(None, unusable),
		];
		let classified: Vec<_> =
			cases.iter().map(|(follow_end, error)| classify(*follow_end, error)).collect();

		assert_eq!(
			classified,
			vec![
				unavailable(&cases[0].1),
				unavailable(&cases[1].1),
				failed(&cases[2].1),
				unavailable(&cases[3].1),
				unavailable(&cases[4].1),
				failed(&cases[5].1),
			]
		);
	}

	/// The halt the retries exist for: the host keeps refusing to follow
	/// until the pauses run out, and the product is told to ask again.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_refused_until_the_pauses_run_out_is_unavailable() {
		let (read, timer) = read_current_block(ScriptedChainHead {
			refused_follows: usize::MAX,
			..ScriptedChainHead::default()
		});

		// The first sleep is the read's deadline, and each later one a pause.
		for (attempt, pause) in REFUSED_FOLLOW_PAUSES.into_iter().enumerate() {
			wait_until(|| timer.sleeps() == attempt + 2, "the next pause starts");
			timer.advance(pause);
		}

		let error = read.join().unwrap().expect_err("no follow is accepted");
		assert!(matches!(error, CurrentBlockError::Unavailable(_)), "{error:?}");
	}

	/// A provider that cannot serve the follow at all will never be able to,
	/// so asking again would only repeat the failure.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_the_provider_cannot_serve_is_failed() {
		let (read, _timer) = read_current_block(ScriptedChainHead {
			refused_follows: usize::MAX,
			refusal_code: -32601,
			..ScriptedChainHead::default()
		});

		let error = read.join().unwrap().expect_err("no follow is accepted");
		assert!(matches!(error, CurrentBlockError::Failed(_)), "{error:?}");
	}

	/// Subxt keeps waiting on a runtime call whose follow stopped, and so does
	/// a node that never answers one. Only the deadline ends that wait.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_runtime_call_that_never_finishes_is_unavailable_at_the_deadline() {
		let timer = Arc::new(ManualTimer::default());
		let (provider, chain) =
			ScriptedChainHead { finishes_calls: false, ..ScriptedChainHead::default() }.serve();
		let chain = chain.with_timer(timer.clone());
		let read = thread::spawn(move || block_on(chain.current_block(&GENESIS)).map(drop));

		wait_for_sent(&provider, |sent| {
			sent.iter().any(|request| request.contains("chainHead_v1_call"))
		});
		timer.advance(CURRENT_BLOCK_TIMEOUT);

		assert_eq!(
			read.join().unwrap(),
			Err(CurrentBlockError::Unavailable(format!(
				"the node gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
			)))
		);
	}

	/// A chain the host cannot connect to, such as one it serves no provider
	/// for, may never become reachable, so it is told apart from a node that
	/// could not be asked.
	#[test]
	fn a_chain_the_host_cannot_connect_to_is_no_connection() {
		struct NoProvider;

		#[async_trait]
		impl RuntimeChainProvider for NoProvider {
			async fn connect(
				&self,
				_genesis_hash: Vec<u8>,
			) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
				Err(RuntimeFailure::unavailable("remote_chain_connect"))
			}
		}

		let chain = ChainRuntime::new(Arc::new(NoProvider), test_spawner());

		let error =
			block_on(chain.current_block(&GENESIS)).expect_err("the chain cannot be connected");

		assert!(matches!(error, CurrentBlockError::NoConnection(_)), "{error:?}");
	}

	#[test]
	fn the_deadline_outlasts_every_pause() {
		// A follow accepted on its last chance must still be able to serve the
		// read before the deadline ends it.
		let pauses: Duration = REFUSED_FOLLOW_PAUSES.iter().sum();
		assert!(CURRENT_BLOCK_TIMEOUT > pauses, "{CURRENT_BLOCK_TIMEOUT:?} <= {pauses:?}");
	}
}
