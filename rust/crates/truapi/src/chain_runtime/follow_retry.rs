//! RPC client for the core's Subxt backend that subscribes to
//! `chainHead_v1_follow` again when the provider refuses it, and records why a
//! follow ended for good.

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
	chain_halt::{ask_through_halts, says_chain_halted},
	runtime_call_client::RuntimeCallClient,
};
use truapi::CancellationToken;

use crate::time::Timer;

const CHAIN_HEAD_FOLLOW: &str = "chainHead_v1_follow";

/// Why a follow sent through [`FollowRetryingRpcClient`] ended for good.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowEnd {
	/// The provider said the chain halted, and still did after every pause.
	Refused,
	/// The provider refused it for a reason other than a halt, which asking
	/// again cannot change.
	Rejected,
	/// The transport failed, or the follow failed after it started.
	Dropped,
	/// The core ended it because a read on it got no answer in time.
	Stalled,
}

/// [`RuntimeCallClient`] that asks again when the provider refuses a chainHead
/// follow.
///
/// Subxt ends its follow for good when a re-follow fails, and every call on
/// that backend fails with it. A provider can refuse a follow and accept one
/// moments later, as a host does while it waits to redial a halted chain.
/// Asking again turns the refusal into a slower re-follow, so calls waiting on
/// the follow keep waiting. Only a refusal saying the chain halted is asked
/// again. Any other refusal and a transport failure reach Subxt at once, as
/// does a halt that outlasts every pause, and Subxt ends the follow.
///
/// Subxt reports any end as a dropped subscription, so this client records the
/// cause in [`Self::follow_end`] before the failure reaches Subxt. It also
/// counts the requests a halt answered in [`Self::halts`], since Subxt keeps
/// only the text of some of those failures.
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

	/// Why this client's follow ended, set before Subxt learns of the end, so
	/// a call that saw the follow drop always finds it here.
	pub fn follow_end(&self) -> Arc<OnceLock<FollowEnd>> {
		self.follow_end.clone()
	}

	/// `subscription`, recording a failure of it, which ends the follow for
	/// Subxt.
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
			let result = ask_through_halts(&*self.timer, &CancellationToken::default(), || {
				self.inner.subscribe_raw(sub, params.clone(), unsub)
			})
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
		chain_runtime::{ChainRuntime, chain_halt::HALT_PAUSES},
		test_support::{FollowRefusal, ScriptedChainHead, follow_count, wait_until},
	};

	const GENESIS: [u8; 32] = [0xbb; 32];

	/// Select the current block on its own thread, so the test can move the
	/// clock while the read waits on the follow.
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

	/// A refused follow must not fail the calls waiting on it, such as a
	/// transaction build reading the current block's metadata. They wait for
	/// the next follow instead.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_call_waiting_on_a_refused_follow_succeeds_on_the_next_one() {
		let (provider, chain, timer) =
			ScriptedChainHead { refused_follows: 1, ..ScriptedChainHead::default() }.serve();
		let read = select_current_block(&chain);

		wait_until(|| timer.sleeps() == 1, "the first pause starts");
		timer.advance(HALT_PAUSES[0]);

		read.join().unwrap().expect("the second follow serves the block");
		assert_eq!(follow_count(&provider), 2);
	}

	/// Each refusal is followed by the next pause, spaced to land after
	/// dotli's own redials, so the schedule is spelled out here rather than
	/// read back from the constant. Once the pauses run out the follow ends,
	/// so a chain that stays halted does not hold calls open.
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
			assert_eq!((follow_count(&provider), timer.remaining()), (attempt + 1, vec![pause]));
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
		assert_eq!((follow_count(&provider), timer.sleeps()), (schedule.len() + 1, schedule.len()));
	}

	/// Only a halt is worth waiting out. A host refusing for any other reason,
	/// even with the internal-error code dotli also uses for a halt, would only
	/// hold every core call through the pauses before failing it anyway.
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

		assert_eq!((follow_count(&provider), timer.sleeps()), (1, 0));
	}
}
