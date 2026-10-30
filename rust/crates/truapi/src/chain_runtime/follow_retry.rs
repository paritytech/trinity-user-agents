//! RPC client for the core's Subxt backend that retries a follow refused by a
//! halt and records why a follow ended.

use std::sync::{
	Arc, OnceLock,
	atomic::{AtomicUsize, Ordering},
};

use futures::{StreamExt, TryFutureExt};
use serde_json::value::RawValue;
use subxt_rpcs::{
	Error as RpcError,
	client::{RawRpcFuture, RawRpcSubscription, RpcClientT},
};

use super::{
	chain_halt::{retry_through_halts, says_chain_halted},
	runtime_call_client::RuntimeCallClient,
};
use truapi::CancellationToken;

use crate::time::Timer;

const CHAIN_HEAD_FOLLOW: &str = "chainHead_v1_follow";

/// Why a follow sent through [`FollowRetryingRpcClient`] ended for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowEnd {
	/// The chain was still halted after every pause.
	Refused,
	/// Refused for a reason other than a halt.
	Rejected,
	/// The transport failed, or the follow failed after it started.
	Dropped,
	/// The core ended it after a read on it timed out.
	Stalled,
}

/// [`RuntimeCallClient`] that retries a chainHead follow refused by a halt.
///
/// Subxt gives up on its backend after one failed re-follow, so without this
/// a brief halt fails every call waiting on the follow. Other refusals pass
/// through at once.
///
/// Subxt reports every follow end as a dropped subscription, so this client
/// records the real cause in [`Self::follow_end`] and counts halted requests
/// in [`Self::halts`].
pub struct FollowRetryingRpcClient {
	inner: RuntimeCallClient,
	timer: Arc<dyn Timer>,
	follow_end: Arc<OnceLock<FollowEnd>>,
	halts: Arc<AtomicUsize>,
}

impl FollowRetryingRpcClient {
	/// Retry refused follows sent through `inner`, pausing on `timer`.
	pub fn new(inner: RuntimeCallClient, timer: Arc<dyn Timer>) -> Self {
		Self { inner, timer, follow_end: Arc::default(), halts: Arc::default() }
	}

	/// How many requests sent through this client a halt has answered.
	pub fn halts(&self) -> Arc<AtomicUsize> {
		self.halts.clone()
	}

	/// Why the follow ended. Set before Subxt sees the end, so a failed call
	/// always finds it.
	pub fn follow_end(&self) -> Arc<OnceLock<FollowEnd>> {
		self.follow_end.clone()
	}

	/// `subscription`, recording an error on it as [`FollowEnd::Dropped`].
	fn recording_failures(&self, subscription: RawRpcSubscription) -> RawRpcSubscription {
		let follow_end = self.follow_end.clone();
		RawRpcSubscription {
			stream: subscription
				.stream
				.inspect(move |item| {
					if item.is_err() {
						let _ = follow_end.set(FollowEnd::Dropped);
					}
				})
				.boxed(),
			id: subscription.id,
		}
	}
}

impl RpcClientT for FollowRetryingRpcClient {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		let halts = self.halts.clone();
		Box::pin(self.inner.request_raw(method, params).inspect_err(move |error| {
			if says_chain_halted(error) {
				halts.fetch_add(1, Ordering::SeqCst);
			}
		}))
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<RawValue>>,
		unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		if sub != CHAIN_HEAD_FOLLOW {
			return self.inner.subscribe_raw(sub, params, unsub);
		}
		Box::pin(async move {
			let result = retry_through_halts(
				&*self.timer,
				&CancellationToken::default(),
				|| self.inner.subscribe_raw(sub, params.clone(), unsub),
				says_chain_halted,
			)
			.await;
			let ended = match &result {
				Ok(_) => return result.map(|subscription| self.recording_failures(subscription)),
				Err(error) if says_chain_halted(error) => FollowEnd::Refused,
				Err(RpcError::User(_)) => FollowEnd::Rejected,
				Err(_) => FollowEnd::Dropped,
			};
			let _ = self.follow_end.set(ended);
			result
		})
	}
}

#[cfg(test)]
mod tests {
	use core::time::Duration;
	use std::thread;

	use futures::executor::block_on;
	use subxt::error::{BackendError, OnlineClientAtBlockError, RpcError as SubxtRpcError};

	use crate::{
		chain_runtime::{ChainRuntime, chain_halt::halt_schedule},
		test_support::{FollowRefusal, ScriptedChainHead, sent_count, wait_until},
	};

	const GENESIS: [u8; 32] = [0xbb; 32];

	/// Read on a separate thread, so the test can advance the clock.
	fn select_current_block(
		chain: &ChainRuntime,
	) -> thread::JoinHandle<Result<(), Box<OnlineClientAtBlockError>>> {
		let chain = chain.clone();
		thread::spawn(move || {
			block_on(async {
				let client = chain.online_client(&GENESIS).await.expect("client");
				client.at_current_block().await.map(drop).map_err(Box::new)
			})
		})
	}

	/// Calls waiting on a refused follow wait for the next one instead of
	/// failing.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_call_waiting_on_a_refused_follow_succeeds_on_the_next_one() {
		let (provider, chain, timer) =
			ScriptedChainHead { refused_follows: 1, ..ScriptedChainHead::default() }.serve();
		let read = select_current_block(&chain);

		wait_until(|| timer.sleeps() == 1, "the first pause starts");
		timer.advance(halt_schedule()[0]);

		read.join().unwrap().expect("the second follow serves the block");
		assert_eq!(sent_count(&provider, "chainHead_v1_follow"), 2);
	}

	/// The schedule is spelled out rather than read from the constant, since
	/// it has to track dotli's redials. Once it runs out the follow ends.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_refused_follow_is_asked_again_after_each_pause_until_they_run_out() {
		let (provider, chain, timer) =
			ScriptedChainHead { refused_follows: usize::MAX, ..ScriptedChainHead::default() }
				.serve();
		let read = select_current_block(&chain);

		let schedule = [1, 2, 4, 8, 16].map(Duration::from_secs);
		for (attempt, pause) in schedule.into_iter().enumerate() {
			wait_until(|| timer.sleeps() == attempt + 1, "the next pause starts");
			assert_eq!(
				(sent_count(&provider, "chainHead_v1_follow"), timer.remaining()),
				(attempt + 1, vec![pause])
			);
			timer.advance(pause);
		}

		let error = read.join().unwrap().expect_err("no follow is accepted");
		assert!(
			matches!(
				*error,
				OnlineClientAtBlockError::CannotGetCurrentBlock {
					reason: BackendError::Rpc(SubxtRpcError::SubscriptionDropped)
				}
			),
			"{error}"
		);
		assert_eq!(
			(sent_count(&provider, "chainHead_v1_follow"), timer.sleeps()),
			(schedule.len() + 1, schedule.len())
		);
	}

	/// Only a halt is retried, even when another refusal uses the same error
	/// code.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_refusal_that_does_not_say_the_chain_halted_is_not_asked_again() {
		let (provider, chain, timer) = ScriptedChainHead {
			refused_follows: usize::MAX,
			refusal: FollowRefusal::Error(-32603),
			..ScriptedChainHead::default()
		}
		.serve();

		select_current_block(&chain).join().unwrap().expect_err("no follow is accepted");

		assert_eq!((sent_count(&provider, "chainHead_v1_follow"), timer.sleeps()), (1, 0));
	}
}
