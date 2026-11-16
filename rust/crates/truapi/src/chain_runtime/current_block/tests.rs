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
	Spawner,
	chain_runtime::{RuntimeChainProvider, chain_halt::halt_schedule},
	platform::JsonRpcConnection,
	test_support::{
		FollowRefusal, ScriptedChainHead, ScriptedProvider, sent_count, wait_for_sent, wait_until,
	},
	time::ManualTimer,
};

const GENESIS: [u8; 32] = [0xbb; 32];

/// Read on a separate thread, so the test can advance the clock.
fn read_current_block(chain: &ChainRuntime) -> thread::JoinHandle<Result<(), CurrentBlockError>> {
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
	let unavailable =
		|error: &OnlineClientAtBlockError| CurrentBlockError::ChainUnavailable(error.to_string());
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
		ScriptedChainHead { halted_headers: usize::MAX, ..ScriptedChainHead::default() }.serve();
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
		ScriptedChainHead { refused_follows: usize::MAX, ..ScriptedChainHead::default() }.serve();
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
		ChainRuntime::new(Arc::new(NeverConnects), Spawner::for_test()).with_timer(timer.clone());
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
		ChainRuntime::new(Arc::new(NoProvider), Spawner::for_test()).with_timer(timer.clone());

	let error = block_on(chain.current_block(&GENESIS)).expect_err("the chain cannot be connected");

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
	let provider =
		Arc::new(UnreachableAtFirst { attempts: AtomicUsize::new(0), unreachable_for: 2, node });
	let timer = Arc::new(ManualTimer::default());
	let chain = ChainRuntime::new(provider.clone(), Spawner::for_test()).with_timer(timer.clone());
	let read = read_current_block(&chain);

	for (pause_index, pause) in halt_schedule().into_iter().take(2).enumerate() {
		wait_until(|| timer.sleeps() == pause_index + 2, "the next pause starts");
		timer.advance(pause);
	}

	read.join().unwrap().expect("the node is read once it can be reached");
	assert_eq!(provider.attempts.load(Ordering::Relaxed), 3);
}

/// A node still out of reach after the last pause is `ChainUnavailable`,
/// so the product may retry.
#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn a_node_never_reached_is_unavailable_after_the_last_pause() {
	struct Unreachable;

	#[async_trait]
	impl RuntimeChainProvider for Unreachable {
		async fn connect(
			&self,
			_genesis_hash: Vec<u8>,
		) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
			Err(RuntimeFailure::unavailable("remote_chain_connect"))
		}
	}

	let timer = Arc::new(ManualTimer::default());
	let chain =
		ChainRuntime::new(Arc::new(Unreachable), Spawner::for_test()).with_timer(timer.clone());
	let read = read_current_block(&chain);

	for (pause_index, pause) in halt_schedule().into_iter().enumerate() {
		wait_until(|| timer.sleeps() == pause_index + 2, "the next pause starts");
		timer.advance(pause);
	}

	let error = read.join().unwrap().expect_err("the node is never reached");
	assert!(matches!(error, CurrentBlockError::ChainUnavailable(_)), "{error:?}");
}

/// A phone that loses its network closes the connection under the read.
/// After a pause the read is tried again on a new connection.
#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn a_read_whose_connection_closes_under_it_is_read_again_on_a_new_one() {
	let (provider, chain, timer) =
		ScriptedChainHead { unanswered_headers: 1, ..ScriptedChainHead::default() }.serve();
	let read = read_current_block(&chain);

	wait_for_sent(&provider, |sent: &[String]| {
		sent.iter().any(|request| request.contains("chainHead_v1_header"))
	});
	provider.sender.lock().unwrap().take();
	wait_until(|| timer.sleeps() == 2, "the pause starts");
	timer.advance(halt_schedule()[0]);

	read.join().unwrap().expect("the read on the new connection is served");
	assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 2);
	assert_eq!(sent_count(&provider, "chainHead_v1_header"), 2);
}

#[test]
fn the_deadline_outlasts_every_pause() {
	// A follow accepted on its last chance must still be able to serve the
	// read before the deadline ends it.
	let pauses: Duration = halt_schedule().iter().sum();
	assert!(CURRENT_BLOCK_TIMEOUT > pauses, "{CURRENT_BLOCK_TIMEOUT:?} <= {pauses:?}");
}
