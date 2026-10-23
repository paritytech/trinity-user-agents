//! RPC client for the core's Subxt backend that subscribes to
//! `chainHead_v1_follow` again when the provider refuses it.

use core::time::Duration;

use serde_json::value::RawValue;
use subxt_rpcs::{
	Error as RpcError,
	client::{RawRpcFuture, RawRpcSubscription, RpcClientT},
};
use tracing::warn;

use crate::host_rpc_client::HostRpcClient;

const CHAIN_HEAD_FOLLOW: &str = "chainHead_v1_follow";

/// JSON-RPC errors that reject the request itself, which asking again cannot
/// change.
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;

/// Pauses before each new follow after a refusal. dotli lets the first redial
/// of a halted chain through at once, then waits one second and doubles the
/// wait each time. These pauses double the same way, so each attempt comes
/// after another chance to redial.
#[cfg(not(test))]
const REFUSED_FOLLOW_PAUSES: [Duration; 5] = [
	Duration::from_secs(1),
	Duration::from_secs(2),
	Duration::from_secs(4),
	Duration::from_secs(8),
	Duration::from_secs(16),
];
#[cfg(test)]
const REFUSED_FOLLOW_PAUSES: [Duration; 5] = [Duration::from_millis(1); 5];

/// [`HostRpcClient`] that asks again when the provider refuses a chainHead
/// follow.
///
/// Subxt ends its follow for good when a re-follow fails, and every call on
/// that backend fails with it. A provider can refuse a follow and accept one
/// moments later, as a host does while it waits to redial a halted chain.
/// Asking again turns the refusal into a slower re-follow, so calls waiting on
/// the follow keep waiting. A transport failure is not retried, nor is an
/// error saying the provider cannot serve the request at all, and a refusal
/// that outlasts every pause reaches Subxt, which ends the follow.
pub struct FollowRetryingRpcClient {
	inner: HostRpcClient,
}

impl FollowRetryingRpcClient {
	/// Retry refused follows sent through `inner`.
	pub fn new(inner: HostRpcClient) -> Self {
		Self { inner }
	}
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
		Box::pin(async move {
			let mut pauses = REFUSED_FOLLOW_PAUSES.into_iter();
			loop {
				match self.inner.subscribe_raw(sub, params.clone(), unsub).await {
					Err(RpcError::User(refusal))
						if !matches!(refusal.code, METHOD_NOT_FOUND | INVALID_PARAMS) =>
					{
						let Some(pause) = pauses.next() else {
							return Err(RpcError::User(refusal));
						};
						warn!(?pause, %refusal, "chainHead follow refused; asking again");
						futures_timer::Delay::new(pause).await;
					},
					result => return result,
				}
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use futures::executor::block_on;
	use subxt::error::{BackendError, OnlineClientAtBlockError, RpcError as SubxtRpcError};

	use super::{METHOD_NOT_FOUND, REFUSED_FOLLOW_PAUSES};
	use crate::test_support::{chain_refusing_follows, follow_count};

	const GENESIS: [u8; 32] = [0xbb; 32];
	/// The server error dotli answers a follow with while a chain is halted.
	const CHAIN_HALTED: i32 = -32000;

	/// A refused follow must not fail the calls waiting on it, such as a
	/// transaction build reading the current block's metadata. They wait for
	/// the next follow instead.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_call_waiting_on_a_refused_follow_succeeds_on_the_next_one() {
		let (provider, chain) = chain_refusing_follows(1, CHAIN_HALTED);
		let client = block_on(chain.online_client(&GENESIS)).expect("client");

		block_on(client.at_current_block()).expect("the second follow serves the block");

		assert_eq!(follow_count(&provider), 2);
	}

	/// A chain that stays halted must not hold calls open: once every pause
	/// has been tried, the follow ends and the waiting call fails.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_refused_after_every_pause_ends() {
		let (provider, chain) = chain_refusing_follows(usize::MAX, CHAIN_HALTED);
		let client = block_on(chain.online_client(&GENESIS)).expect("client");

		let error = block_on(client.at_current_block()).expect_err("no follow is accepted");

		assert!(
			matches!(
				error,
				OnlineClientAtBlockError::CannotGetCurrentBlock {
					reason: BackendError::Rpc(SubxtRpcError::SubscriptionDropped)
				}
			),
			"{error}"
		);
		assert_eq!(follow_count(&provider), REFUSED_FOLLOW_PAUSES.len() + 1);
	}

	/// A provider that does not serve chainHead at all will never accept a
	/// follow, so waiting out the pauses would only delay every core call.
	#[cfg_attr(target_arch = "wasm32", ignore)]
	#[test]
	fn a_follow_the_provider_cannot_serve_is_not_asked_again() {
		let (provider, chain) = chain_refusing_follows(usize::MAX, METHOD_NOT_FOUND);
		let client = block_on(chain.online_client(&GENESIS)).expect("client");

		block_on(client.at_current_block()).expect_err("no follow is accepted");

		assert_eq!(follow_count(&provider), 1);
	}
}
