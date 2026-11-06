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
	ChainRuntime, ConnectionScope, RuntimeFailure, RuntimeFailureKind,
	chain_halt::retry_while_halted, follow_retry::FollowEnd,
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
	/// No connection could be opened, for instance to a chain the host does
	/// not serve.
	#[display("{_0}")]
	NoChainConnection(RuntimeFailure),
	/// The chain stayed halted, the follow dropped, or the node did not answer
	/// in time. A later call can succeed.
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
		let no_connection = |failure: RuntimeFailure| FailedRead {
			retryable: failure.kind() == RuntimeFailureKind::Unavailable,
			error: CurrentBlockError::NoChainConnection(failure),
		};
		let read_block = || async move {
			debug_assert!(
				matches!(self.scope, ConnectionScope::Core),
				"the Subxt backend's driver task defeats release_scope on a product scope",
			);
			let connection = self
				.connection_for("subxt_connection", genesis_hash)
				.await
				.map_err(no_connection)?;
			let subxt = connection.subxt_connection().await.map_err(no_connection)?;
			let halts = subxt.follow.halts();
			subxt.client.at_current_block().await.map_err(|error| {
				let halted = subxt.follow.halts() > halts;
				// Subxt reports some steps, such as the metadata download, only
				// as text, so a closed connection counts as a dropped follow.
				let follow_end = subxt
					.follow
					.end()
					.or_else(|| connection.is_closed().then_some(FollowEnd::Dropped));
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
mod tests {
	use std::{
		sync::{
			Arc,
			atomic::{AtomicUsize, Ordering},
		},
		thread,
	};

	use async_trait::async_trait;
	use futures::{executor::block_on, future};
	use subxt::error::{BackendError, Hex, RpcError as SubxtRpcError};

	use super::*;
	use crate::{
		chain_runtime::{RuntimeChainProvider, chain_halt::halt_schedule},
		platform::JsonRpcConnection,
		test_support::{
			FollowRefusal, ScriptedChainHead, ScriptedProvider, sent_count, test_spawner,
			wait_for_sent, wait_until,
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

	fn deadline_error() -> CurrentBlockError {
		CurrentBlockError::ChainUnavailable(format!(
			"the chain gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
		))
	}

	/// Products retry on `ChainUnavailable`, so it must only cover failures a later
	/// call can get past. Unusable metadata stays `Other` even during a halt.
	#[test]
	fn a_failure_is_retryable_only_when_a_later_call_can_get_past_it() {
		let dropped = || OnlineClientAtBlockError::CannotGetCurrentBlock {
			reason: BackendError::Rpc(SubxtRpcError::SubscriptionDropped),
		};
		let unavailable = |error: &OnlineClientAtBlockError| {
			CurrentBlockError::ChainUnavailable(error.to_string())
		};
		let other = |error: &OnlineClientAtBlockError| CurrentBlockError::Other(error.to_string());
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
			(Some(FollowEnd::Halted), false, dropped()),
			(Some(FollowEnd::Dropped), false, dropped()),
			(Some(FollowEnd::Refused), false, dropped()),
			(None, false, dropped()),
			(None, true, metadata_fetch()),
			(Some(FollowEnd::Halted), false, metadata_fetch()),
			(None, false, metadata_fetch()),
			(None, false, stale_block),
			(None, false, unusable()),
			(Some(FollowEnd::Dropped), true, unusable()),
		];
		let classified: Vec<_> = cases
			.iter()
			.map(|(follow_end, halted, error)| classify(*follow_end, *halted, error))
			.collect();

		assert_eq!(
			classified,
			vec![
				unavailable(&cases[0].2),
				unavailable(&cases[1].2),
				other(&cases[2].2),
				other(&cases[3].2),
				unavailable(&cases[4].2),
				unavailable(&cases[5].2),
				other(&cases[6].2),
				unavailable(&cases[7].2),
				other(&cases[8].2),
				other(&cases[9].2),
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
		assert_eq!(timer.remaining(), vec![CURRENT_BLOCK_TIMEOUT, halt_schedule()[0]]);
		timer.advance(halt_schedule()[0]);

		read.join().unwrap().expect("the read after the pause is served");
		assert_eq!(sent_count(&provider, "chainHead_v1_header"), 2);
	}

	/// A chain that stays halted is `ChainUnavailable`, so the product may retry.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_read_still_halted_after_every_pause_is_unavailable() {
		let (_provider, chain, timer) =
			ScriptedChainHead { halted_headers: usize::MAX, ..ScriptedChainHead::default() }
				.serve();
		let read = read_current_block(&chain);

		for (pause_index, pause) in halt_schedule().into_iter().enumerate() {
			wait_until(|| timer.sleeps() == pause_index + 2, "the next pause starts");
			timer.advance(pause);
		}

		let error = read.join().unwrap().expect_err("the chain stays halted");
		assert!(matches!(error, CurrentBlockError::ChainUnavailable(_)), "{error:?}");
	}

	/// A follow refused until the pauses run out is `ChainUnavailable`.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_refused_until_the_pauses_run_out_is_unavailable() {
		let (_provider, chain, timer) =
			ScriptedChainHead { refused_follows: usize::MAX, ..ScriptedChainHead::default() }
				.serve();
		let read = read_current_block(&chain);

		// The first sleep is the read's deadline, and each later one a pause.
		for (attempt, pause) in halt_schedule().into_iter().enumerate() {
			wait_until(|| timer.sleeps() == attempt + 2, "the next pause starts");
			timer.advance(pause);
		}

		let error = read.join().unwrap().expect_err("no follow is accepted");
		assert!(matches!(error, CurrentBlockError::ChainUnavailable(_)), "{error:?}");
	}

	/// A refusal that is not a halt will happen again, so it is not retryable.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_refused_for_another_reason_is_not_retryable() {
		let (_provider, chain, _timer) = ScriptedChainHead {
			refused_follows: usize::MAX,
			refusal: FollowRefusal::Error(-32001),
			..ScriptedChainHead::default()
		}
		.serve();

		let error = read_current_block(&chain).join().unwrap().expect_err("no follow is accepted");

		assert!(matches!(error, CurrentBlockError::Other(_)), "{error:?}");
	}

	/// The follow is shared with the core's transaction watches and Bulletin
	/// reads, so a read that misses the deadline gives up alone and leaves the
	/// follow running for them.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_read_that_misses_the_deadline_leaves_the_follow_to_its_other_users() {
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
		assert_eq!(first.join().unwrap(), Err(deadline_error()));

		let second = read_current_block(&chain);
		wait_for_sent(&provider, calls_started(2));
		let follows = ["chainHead_v1_follow", "chainHead_v1_unfollow"]
			.map(|method| sent_count(&provider, method));
		assert_eq!(follows, [1, 0]);
		timer.advance(CURRENT_BLOCK_TIMEOUT);
		assert_eq!(second.join().unwrap(), Err(deadline_error()));
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

		assert_eq!(read.join().unwrap(), Err(deadline_error()));
	}

	/// A chain the host does not serve is told apart from a halted one, and
	/// reported at once, since asking again gets the same answer.
	#[test]
	fn a_chain_the_host_does_not_serve_is_no_connection_at_once() {
		struct NoProvider;

		#[async_trait]
		impl RuntimeChainProvider for NoProvider {
			async fn connect(
				&self,
				_genesis_hash: Vec<u8>,
			) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
				Err(RuntimeFailure::host_failure("remote_chain_connect", "not served"))
			}
		}

		let timer = Arc::new(ManualTimer::default());
		let chain =
			ChainRuntime::new(Arc::new(NoProvider), test_spawner()).with_timer(timer.clone());

		let error =
			block_on(chain.current_block(&GENESIS)).expect_err("the chain cannot be connected");

		assert!(matches!(error, CurrentBlockError::NoChainConnection(_)), "{error:?}");
		assert_eq!(timer.sleeps(), 1, "only the deadline started, no pause");
	}

	/// A phone that loses its network cannot reach the node for a while. The
	/// read waits that out instead of failing the transaction.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_node_that_cannot_be_reached_for_a_while_is_read_once_it_can() {
		struct UnreachableAtFirst {
			attempts: AtomicUsize,
			unreachable_for: usize,
			node: Arc<ScriptedProvider>,
		}

		#[async_trait]
		impl RuntimeChainProvider for UnreachableAtFirst {
			async fn connect(
				&self,
				genesis_hash: Vec<u8>,
			) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
				if self.attempts.fetch_add(1, Ordering::Relaxed) < self.unreachable_for {
					return Err(RuntimeFailure::unavailable("remote_chain_connect"));
				}
				self.node.connect(genesis_hash).await
			}
		}

		let (node, _, _) = ScriptedChainHead::default().serve();
		let provider = Arc::new(UnreachableAtFirst {
			attempts: AtomicUsize::new(0),
			unreachable_for: 2,
			node,
		});
		let timer = Arc::new(ManualTimer::default());
		let chain = ChainRuntime::new(provider.clone(), test_spawner()).with_timer(timer.clone());
		let read = read_current_block(&chain);

		for (pause_index, pause) in halt_schedule().into_iter().take(2).enumerate() {
			wait_until(|| timer.sleeps() == pause_index + 2, "the next pause starts");
			timer.advance(pause);
		}

		read.join().unwrap().expect("the node is read once it can be reached");
		assert_eq!(provider.attempts.load(Ordering::Relaxed), 3);
	}

	#[test]
	fn the_deadline_outlasts_every_pause() {
		// A follow accepted on its last chance must still be able to serve the
		// read before the deadline ends it.
		let pauses: Duration = halt_schedule().iter().sum();
		assert!(CURRENT_BLOCK_TIMEOUT > pauses, "{CURRENT_BLOCK_TIMEOUT:?} <= {pauses:?}");
	}
}
