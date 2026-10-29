//! RPC client for the core's Subxt backend that subscribes to
//! `chainHead_v1_follow` again when the provider refuses it, and records why a
//! follow ended for good.

use core::{ops::ControlFlow, time::Duration};
use std::sync::{Arc, OnceLock};

use futures::StreamExt;
use serde_json::value::RawValue;
use subxt_rpcs::{
	Error as RpcError, UserError,
	client::{RawRpcFuture, RawRpcSubscription, RpcClientT},
};
use tracing::warn;

use super::runtime_call_client::RuntimeCallClient;
use crate::time::{Timer, retry_on};

const CHAIN_HEAD_FOLLOW: &str = "chainHead_v1_follow";

/// The `error.data` dotli answers a follow with while it waits to redial a
/// halted chain, which it documents as worth asking again.
const CHAIN_HALTED: &str = "dotli:chain-halted";

/// Pauses before each new follow after a refusal. dotli lets the first redial
/// of a halted chain through at once, then waits one second and doubles the
/// wait each time. These pauses double the same way, so each attempt comes
/// after another chance to redial.
pub const REFUSED_FOLLOW_PAUSES: [Duration; 5] = [
	Duration::from_secs(1),
	Duration::from_secs(2),
	Duration::from_secs(4),
	Duration::from_secs(8),
	Duration::from_secs(16),
];

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
/// cause in [`Self::follow_end`] before the failure reaches Subxt.
pub struct FollowRetryingRpcClient {
	inner: RuntimeCallClient,
	timer: Arc<dyn Timer>,
	follow_end: Arc<OnceLock<FollowEnd>>,
}

impl FollowRetryingRpcClient {
	/// Retry refused follows sent through `inner`, pausing on `timer`.
	pub fn new(inner: RuntimeCallClient, timer: Arc<dyn Timer>) -> Self {
		Self { inner, timer, follow_end: Arc::default() }
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

/// Whether `refusal` says the chain halted, so a later follow can be accepted.
fn says_chain_halted(refusal: &UserError) -> bool {
	refusal.data.as_deref().is_some_and(|data| {
		serde_json::from_str::<&str>(data.get()).is_ok_and(|data| data == CHAIN_HALTED)
	})
}

impl RpcClientT for FollowRetryingRpcClient {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		self.inner.request_raw(method, params)
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
		let mut pauses = REFUSED_FOLLOW_PAUSES.into_iter();
		Box::pin(retry_on(
			&*self.timer,
			move || {
				let pause = pauses.next();
				let params = params.clone();
				async move {
					let (ended, error) = match self.inner.subscribe_raw(sub, params, unsub).await {
						Ok(subscription) => {
							return ControlFlow::Break(Ok(self.recording_failures(subscription)));
						},
						Err(RpcError::User(refusal)) if says_chain_halted(&refusal) => {
							match pause {
								Some(pause) => return ControlFlow::Continue((pause, refusal)),
								None => (FollowEnd::Refused, RpcError::User(refusal)),
							}
						},
						Err(RpcError::User(rejection)) => {
							(FollowEnd::Rejected, RpcError::User(rejection))
						},
						Err(error) => (FollowEnd::Dropped, error),
					};
					let _ = self.follow_end.set(ended);
					ControlFlow::Break(Err(error))
				}
			},
			|(pause, refusal)| {
				warn!(?pause, %refusal, "chainHead follow refused; asking again");
				pause
			},
		))
	}
}

#[cfg(test)]
mod tests {
	use core::time::Duration;
	use std::thread;

	use futures::executor::block_on;
	use subxt::error::{BackendError, OnlineClientAtBlockError, RpcError as SubxtRpcError};

	use super::REFUSED_FOLLOW_PAUSES;
	use crate::{
		chain_runtime::ChainRuntime,
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
		timer.advance(REFUSED_FOLLOW_PAUSES[0]);

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
