//! Reading the current finalized block, for building transactions.

use core::time::Duration;
use std::sync::Mutex;

use derive_more::Display;
use futures::{FutureExt, TryFutureExt, pin_mut};
use subxt::{
	client::OnlineClientAtBlock, config::SubstrateConfig, error::OnlineClientAtBlockError,
};
use tracing::instrument;
use truapi::CancellationToken;

use super::{
	ChainRuntime, RuntimeFailure, SubxtConnection, chain_halt::retry_through_halts,
	follow_retry::FollowEnd,
};

/// Deadline for reading the current block, connecting included. It is longer
/// than all the halt pauses together, so a late re-follow can still serve.
const CURRENT_BLOCK_TIMEOUT: Duration = Duration::from_secs(45);

/// Why [`ChainRuntime::current_block`] could not read the chain.
#[derive(Debug, PartialEq, Eq, Display)]
pub enum CurrentBlockError {
	/// No connection could be opened, for instance to a chain the host does
	/// not serve.
	#[display("{_0}")]
	NoConnection(RuntimeFailure),
	/// The chain stayed halted, the follow dropped, or the node did not answer
	/// in time. A later call can succeed.
	#[display("{_0}")]
	Unavailable(String),
	/// A failure a retry cannot fix, such as metadata this host cannot use.
	#[display("{_0}")]
	Failed(String),
}

impl ChainRuntime {
	/// Subxt client at the current finalized block of `genesis_hash`.
	///
	/// Core scope only, for the same reason as [`Self::online_client`].
	///
	/// A read that meets a halt is retried after each of
	/// [`HALT_PAUSES`](super::chain_halt::HALT_PAUSES).
	///
	/// The deadline exists because Subxt never gives up on a runtime call
	/// whose follow stopped. Missing it ends the follow, so the next call
	/// starts a fresh one instead of waiting on the stuck one.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.current_block"))]
	pub async fn current_block(
		&self,
		genesis_hash: &[u8],
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, CurrentBlockError> {
		let latest_connection = &Mutex::new(None::<SubxtConnection>);
		let never_cancelled = CancellationToken::default();
		let read = retry_through_halts(
			&*self.timer,
			&never_cancelled,
			|| async move {
				let connection = self
					.subxt_connection(genesis_hash)
					.await
					.map_err(|failure| (CurrentBlockError::NoConnection(failure), false))?;
				*latest_connection.lock().unwrap() = Some(connection.clone());
				let halts = connection.halts();
				connection.client.at_current_block().await.map_err(|error| {
					let met_a_halt = connection.halts() > halts;
					(classify(connection.follow_end(), met_a_halt, &error), met_a_halt)
				})
			},
			|(_, met_a_halt)| *met_a_halt,
		)
		.map_err(|(error, _)| error)
		.fuse();
		let deadline = self.timer.sleep(CURRENT_BLOCK_TIMEOUT).fuse();
		pin_mut!(read, deadline);
		futures::select! {
			result = read => result,
			() = deadline => {
				if let Some(connection) = latest_connection.lock().unwrap().take() {
					connection.end_stalled_follow();
				}
				Err(CurrentBlockError::Unavailable(format!(
					"the chain gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
				)))
			},
		}
	}
}

/// Whether a later call can get past `error`. A failure to reach the node is
/// retryable if the read met a halt or the follow ended for a passing reason.
/// Subxt reports every follow end the same way, so `follow_end` decides.
fn classify(
	follow_end: Option<FollowEnd>,
	met_a_halt: bool,
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
			met_a_halt ||
				matches!(
					follow_end,
					Some(FollowEnd::Refused | FollowEnd::Dropped | FollowEnd::Stalled)
				)
		},
		_ => false,
	};
	let reason = error.to_string();
	if retryable {
		CurrentBlockError::Unavailable(reason)
	} else {
		CurrentBlockError::Failed(reason)
	}
}

#[cfg(test)]
mod tests {
	use std::{sync::Arc, thread};

	use async_trait::async_trait;
	use futures::{executor::block_on, future};
	use subxt::error::{BackendError, Hex, RpcError as SubxtRpcError};

	use super::*;
	use crate::{
		chain_runtime::{RuntimeChainProvider, chain_halt::HALT_PAUSES},
		platform::JsonRpcConnection,
		test_support::{
			FollowRefusal, ScriptedChainHead, sent_count, test_spawner, wait_for_sent, wait_until,
		},
		time::ManualTimer,
	};

	const GENESIS: [u8; 32] = [0xbb; 32];

	/// Read on a separate thread, so the test can advance the clock.
	fn read_current_block(
		chain: &ChainRuntime,
	) -> thread::JoinHandle<Result<(), CurrentBlockError>> {
		let chain = chain.clone();
		thread::spawn(move || block_on(chain.current_block(&GENESIS)).map(drop))
	}

	fn no_answer() -> CurrentBlockError {
		CurrentBlockError::Unavailable(format!(
			"the chain gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
		))
	}

	/// Products retry on `Unavailable`, so it must only cover failures a later
	/// call can get past. Unusable metadata stays `Failed` even during a halt.
	#[test]
	fn a_failure_is_retryable_only_when_a_later_call_can_get_past_it() {
		let dropped = || OnlineClientAtBlockError::CannotGetCurrentBlock {
			reason: BackendError::Rpc(SubxtRpcError::SubscriptionDropped),
		};
		let unavailable =
			|error: &OnlineClientAtBlockError| CurrentBlockError::Unavailable(error.to_string());
		let failed =
			|error: &OnlineClientAtBlockError| CurrentBlockError::Failed(error.to_string());
		let metadata_fetch = || OnlineClientAtBlockError::CannotGetMetadata {
			block_hash: Hex::from([1; 32]),
			reason: "subscription dropped".to_string(),
		};
		let stale_block =
			OnlineClientAtBlockError::BlockHeaderNotFound { block_hash: Hex::from([1; 32]) };
		let unusable = || OnlineClientAtBlockError::UnsupportedMetadataVersion {
			block_hash: Hex::from([1; 32]),
			version: 13,
		};

		let cases = [
			(Some(FollowEnd::Refused), false, dropped()),
			(Some(FollowEnd::Dropped), false, dropped()),
			(Some(FollowEnd::Stalled), false, dropped()),
			(Some(FollowEnd::Rejected), false, dropped()),
			(None, false, dropped()),
			(None, true, metadata_fetch()),
			(Some(FollowEnd::Refused), false, metadata_fetch()),
			(None, false, metadata_fetch()),
			(None, false, stale_block),
			(None, false, unusable()),
			(Some(FollowEnd::Dropped), true, unusable()),
		];
		let classified: Vec<_> = cases
			.iter()
			.map(|(follow_end, met_a_halt, error)| classify(*follow_end, *met_a_halt, error))
			.collect();

		assert_eq!(
			classified,
			vec![
				unavailable(&cases[0].2),
				unavailable(&cases[1].2),
				unavailable(&cases[2].2),
				failed(&cases[3].2),
				failed(&cases[4].2),
				unavailable(&cases[5].2),
				unavailable(&cases[6].2),
				failed(&cases[7].2),
				unavailable(&cases[8].2),
				failed(&cases[9].2),
				failed(&cases[10].2),
			]
		);
	}

	/// dotli answers requests in flight with its halt error, then serves them
	/// once the chain is redialed. The read must wait that out.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_read_a_halt_interrupts_is_read_again_after_a_pause() {
		let (provider, chain, timer) =
			ScriptedChainHead { halted_headers: 1, ..ScriptedChainHead::default() }.serve();
		let read = read_current_block(&chain);

		// The first sleep is the read's deadline, the second the pause.
		wait_until(|| timer.sleeps() == 2, "the pause starts");
		assert_eq!(timer.remaining(), vec![CURRENT_BLOCK_TIMEOUT, HALT_PAUSES[0]]);
		timer.advance(HALT_PAUSES[0]);

		read.join().unwrap().expect("the read after the pause is served");
		assert_eq!(sent_count(&provider, "chainHead_v1_header"), 2);
	}

	/// A chain that stays halted is `Unavailable`, so the product may retry.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_read_still_halted_after_every_pause_is_unavailable() {
		let (_provider, chain, timer) =
			ScriptedChainHead { halted_headers: usize::MAX, ..ScriptedChainHead::default() }
				.serve();
		let read = read_current_block(&chain);

		for (pause_index, pause) in HALT_PAUSES.into_iter().enumerate() {
			wait_until(|| timer.sleeps() == pause_index + 2, "the next pause starts");
			timer.advance(pause);
		}

		let error = read.join().unwrap().expect_err("the chain stays halted");
		assert!(matches!(error, CurrentBlockError::Unavailable(_)), "{error:?}");
	}

	/// A follow refused until the pauses run out is `Unavailable`.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_refused_until_the_pauses_run_out_is_unavailable() {
		let (_provider, chain, timer) =
			ScriptedChainHead { refused_follows: usize::MAX, ..ScriptedChainHead::default() }
				.serve();
		let read = read_current_block(&chain);

		// The first sleep is the read's deadline, and each later one a pause.
		for (attempt, pause) in HALT_PAUSES.into_iter().enumerate() {
			wait_until(|| timer.sleeps() == attempt + 2, "the next pause starts");
			timer.advance(pause);
		}

		let error = read.join().unwrap().expect_err("no follow is accepted");
		assert!(matches!(error, CurrentBlockError::Unavailable(_)), "{error:?}");
	}

	/// A refusal that is not a halt will happen again, so it is not retryable.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_refused_for_another_reason_is_failed() {
		let (_provider, chain, _timer) = ScriptedChainHead {
			refused_follows: usize::MAX,
			refusal: FollowRefusal::Error(-32001),
			..ScriptedChainHead::default()
		}
		.serve();

		let error = read_current_block(&chain).join().unwrap().expect_err("no follow is accepted");

		assert!(matches!(error, CurrentBlockError::Failed(_)), "{error:?}");
	}

	/// The deadline ends a stuck read and its follow, so the next call starts
	/// a fresh follow instead of waiting on the same stuck one.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_read_that_misses_the_deadline_ends_its_follow() {
		let (provider, chain, timer) =
			ScriptedChainHead { finishes_calls: false, ..ScriptedChainHead::default() }.serve();
		let calls_started = |count: usize| {
			move |sent: &[String]| {
				sent.iter().filter(|request| request.contains("chainHead_v1_call")).count() == count
			}
		};

		let first = read_current_block(&chain);
		wait_for_sent(&provider, calls_started(1));
		timer.advance(CURRENT_BLOCK_TIMEOUT);
		assert_eq!(first.join().unwrap(), Err(no_answer()));
		wait_for_sent(&provider, |sent| {
			sent.iter().any(|request| request.contains("chainHead_v1_unfollow"))
		});

		let second = read_current_block(&chain);
		wait_for_sent(&provider, calls_started(2));
		assert_eq!(sent_count(&provider, "chainHead_v1_follow"), 2);
		timer.advance(CURRENT_BLOCK_TIMEOUT);
		assert_eq!(second.join().unwrap(), Err(no_answer()));
	}

	/// The deadline also covers a connection that never opens.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_connection_that_never_opens_is_unavailable_at_the_deadline() {
		struct NeverConnects;

		#[async_trait]
		impl RuntimeChainProvider for NeverConnects {
			async fn connect(
				&self,
				_genesis_hash: Vec<u8>,
			) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
				future::pending().await
			}
		}

		let timer = Arc::new(ManualTimer::default());
		let chain =
			ChainRuntime::new(Arc::new(NeverConnects), test_spawner()).with_timer(timer.clone());
		let read = read_current_block(&chain);

		wait_until(|| timer.sleeps() == 1, "the deadline starts");
		timer.advance(CURRENT_BLOCK_TIMEOUT);

		assert_eq!(read.join().unwrap(), Err(no_answer()));
	}

	/// A chain the host cannot connect to is told apart from a halted one.
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
		let pauses: Duration = HALT_PAUSES.iter().sum();
		assert!(CURRENT_BLOCK_TIMEOUT > pauses, "{CURRENT_BLOCK_TIMEOUT:?} <= {pauses:?}");
	}
}
