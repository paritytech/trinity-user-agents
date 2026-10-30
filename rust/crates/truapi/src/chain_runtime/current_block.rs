//! The chain's current finalized block, for building transactions against its
//! runtime metadata.

use core::{ops::ControlFlow, time::Duration};
use std::sync::Mutex;

use derive_more::Display;
use futures::{FutureExt, pin_mut};
use subxt::{
	client::OnlineClientAtBlock, config::SubstrateConfig, error::OnlineClientAtBlockError,
};
use tracing::{instrument, warn};

use super::{
	ChainRuntime, RuntimeFailure, SubxtConnection, chain_halt::HALT_PAUSES, follow_retry::FollowEnd,
};
use crate::time::retry_on;

/// How long reading the current block may take, connecting included: longer
/// than the pauses between refused follows add up to, so a follow accepted at
/// the last attempt still serves the read.
const CURRENT_BLOCK_TIMEOUT: Duration = Duration::from_secs(45);

/// Why [`ChainRuntime::current_block`] could not read the chain.
#[derive(Debug, PartialEq, Eq, Display)]
pub enum CurrentBlockError {
	/// No connection to the chain could be opened, for instance because the
	/// host serves no such chain.
	#[display("{_0}")]
	NoConnection(RuntimeFailure),
	/// The node could not be asked: the chain stayed halted, its follow was
	/// refused until the retries ran out or was dropped, or it gave no answer
	/// in time. A later call can succeed.
	#[display("{_0}")]
	Unavailable(String),
	/// The node answered with nothing this host can use, or its provider
	/// refused the follow for a reason other than a halt.
	#[display("{_0}")]
	Failed(String),
}

impl ChainRuntime {
	/// Subxt client at the current finalized block of `genesis_hash`.
	///
	/// Core scope only, for the same reason as [`Self::online_client`].
	///
	/// A read that a halt interrupts, as when the host answers one of its
	/// requests while it redials the chain, is read again after each of
	/// [`HALT_PAUSES`].
	///
	/// The read has a deadline because a provider can hold the connection
	/// open without completing it, and a runtime call Subxt started on a
	/// follow that then stopped waits for an answer the next follow never
	/// sends. A read that misses the deadline on an open follow ends that
	/// follow, so the node drops what was started on it and the next call
	/// asks a new one.
	#[instrument(skip_all, fields(runtime.method = "chain_runtime.current_block"))]
	pub async fn current_block(
		&self,
		genesis_hash: &[u8],
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, CurrentBlockError> {
		let connected = Mutex::new(None::<SubxtConnection>);
		let mut pauses = HALT_PAUSES.into_iter();
		let read = retry_on(
			&*self.timer,
			|| {
				let pause = pauses.next();
				let connected = &connected;
				async move {
					let connection = match self.subxt_connection(genesis_hash).await {
						Ok(connection) => connection,
						Err(failure) => {
							return ControlFlow::Break(Err(CurrentBlockError::NoConnection(
								failure,
							)));
						},
					};
					*connected.lock().unwrap() = Some(connection.clone());
					let halts = connection.halts();
					let error = match connection.client.at_current_block().await {
						Ok(block) => return ControlFlow::Break(Ok(block)),
						Err(error) => error,
					};
					let met_a_halt = connection.halts() > halts;
					match pause {
						Some(pause) if met_a_halt => ControlFlow::Continue((pause, error)),
						_ => ControlFlow::Break(Err(classify(
							connection.follow_end(),
							met_a_halt,
							&error,
						))),
					}
				}
			},
			|(pause, error)| {
				warn!(?pause, %error, "the chain halted under the current block read; reading again");
				pause
			},
		)
		.fuse();
		let deadline = self.timer.sleep(CURRENT_BLOCK_TIMEOUT).fuse();
		pin_mut!(read, deadline);
		futures::select! {
			result = read => result,
			() = deadline => {
				if let Some(connection) = connected.lock().unwrap().take() {
					connection.end_stalled_follow();
				}
				Err(CurrentBlockError::Unavailable(format!(
					"the chain gave no answer within {CURRENT_BLOCK_TIMEOUT:?}"
				)))
			},
		}
	}
}

/// Sort a failure to select the current block by whether a later call can get
/// past it. An answer this host cannot use fails every time. A failure to ask
/// is worth asking again when a halt answered one of the read's requests, or
/// when the follow ended for a reason that can pass, which only the recorded
/// cause tells, since Subxt reports every end as a dropped subscription.
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
			FollowRefusal, ScriptedChainHead, follow_count, test_spawner, wait_for_sent, wait_until,
		},
		time::ManualTimer,
	};

	const GENESIS: [u8; 32] = [0xbb; 32];

	/// Read the current block on its own thread, so the test can move the
	/// clock while the read waits.
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

	/// A product retries on `Unavailable`, so it must mean a later call can
	/// get past the failure: a halt met the read, or the follow ended for a
	/// reason that can pass. An answer this host cannot use fails every time,
	/// whatever happened to the chain meanwhile.
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

	/// What dotli does to a build in flight when its chain halts: it answers
	/// the read's requests with its halt error, then serves them again once
	/// the chain is redialed. Seen against dotli, the build failed in
	/// milliseconds instead of waiting.
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
		let headers = provider
			.sent
			.lock()
			.unwrap()
			.iter()
			.filter(|request| request.contains("chainHead_v1_header"))
			.count();
		assert_eq!(headers, 2);
	}

	/// A chain that stays halted is reported as one a later call can reach,
	/// not as a failure the product should give up on.
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

	/// The halt the retries exist for: the host keeps refusing to follow
	/// until the pauses run out, and the product is told to ask again.
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

	/// A host that refuses the follow for any reason but a halt, such as a
	/// chain it does not serve, will refuse it again, so the product must not
	/// be told to retry.
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

	/// Subxt keeps waiting on a runtime call whose follow stopped, and so does
	/// a node that never answers one. Only the deadline ends that wait, and
	/// it ends the follow too: otherwise every later call would wait out the
	/// same deadline on the same stuck follow, and each would leave another
	/// operation open on the node.
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
		assert_eq!(follow_count(&provider), 2);
		timer.advance(CURRENT_BLOCK_TIMEOUT);
		assert_eq!(second.join().unwrap(), Err(no_answer()));
	}

	/// A provider can hold a connection open without completing it, as while
	/// it redials a halted chain, and the deadline covers that wait too.
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
		let pauses: Duration = HALT_PAUSES.iter().sum();
		assert!(CURRENT_BLOCK_TIMEOUT > pauses, "{CURRENT_BLOCK_TIMEOUT:?} <= {pauses:?}");
	}
}
