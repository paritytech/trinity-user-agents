//! Runtime helper for People-chain statement-store JSON-RPC.
//!
//! Statement subscriptions open their own host RPC connection for their
//! lifetime instead of riding the shared
//! [`crate::chain_runtime::ChainRuntime`] chainHead runtime. If a host shares
//! same-topic statement subscriptions on one upstream connection, the host
//! broker must fan out and ref-count same-upstream-token notifications.
//!
//! Submissions and the allowance paths carry no statement subscription, so
//! they share the [`ChainRuntime`]'s People-chain connection.

use std::sync::Arc;

use core::{future::Future, time::Duration};
use futures::stream::{self, BoxStream, Stream, StreamExt};
use serde_json::{Value, json};

use crate::platform::{JsonRpcConnection, Platform, ProviderError};
use subxt_rpcs::{
	RpcClient,
	client::{RpcSubscription, rpc_params},
};
use thiserror::Error;
use tracing::warn;

use crate::{
	chain_runtime::{ChainRuntime, RuntimeFailure},
	host_logic::statement_store::{
		NewStatements, SUBMIT_STATEMENT_METHOD, SUBSCRIBE_STATEMENT_METHOD, StatementBatches,
		StatementStoreApi, StatementWatchError, TopicFilter, TopicFilterKind,
		UNSUBSCRIBE_STATEMENT_METHOD, hex_topic, parse_new_statements_result,
		validated_topic_filter,
	},
	host_rpc_client::HostRpcClient,
	stream_extensions::StreamExtensions as _,
	subscription::Spawner,
};

#[cfg(test)]
mod live_people;

#[cfg(test)]
pub use live_people::{LIVE_WAIT, LivePeopleStatements};

/// One attempt plus ten retries, 2 s apart, matching the native iOS and Android hosts.
const SSO_NO_ALLOWANCE_RETRY_ATTEMPTS: usize = 11;
#[cfg(not(test))]
const SSO_NO_ALLOWANCE_RETRY_DELAY: Duration = Duration::from_secs(2);
#[cfg(test)]
const SSO_NO_ALLOWANCE_RETRY_DELAY: Duration = Duration::from_millis(1);

/// Error opening a statement-store RPC client over the host platform.
#[derive(Debug, Error)]
pub enum StatementStoreRpcClientError {
	/// The host failed to open a People-chain JSON-RPC connection.
	#[error("{label} connect failed: {error:?}")]
	Connect {
		/// Operation label requesting the connection.
		label: &'static str,
		/// Chain provider error.
		error: ProviderError,
	},
	/// The pooled People-chain connection could not be opened.
	#[error(transparent)]
	Chain(#[from] RuntimeFailure),
}

/// People-chain statement-store RPC client factory.
#[derive(Clone)]
pub struct StatementStoreRpc {
	platform: Arc<dyn Platform>,
	people_chain_genesis_hash: [u8; 32],
	spawner: Spawner,
	chain: ChainRuntime,
}

impl StatementStoreRpc {
	/// Build a helper that opens subscription connections through
	/// `platform` and sends requests over `chain`'s pooled connection.
	pub fn new(
		platform: Arc<dyn Platform>,
		chain: ChainRuntime,
		people_chain_genesis_hash: [u8; 32],
		spawner: Spawner,
	) -> Self {
		Self { platform, people_chain_genesis_hash, spawner, chain }
	}

	/// Open a People-chain RPC client already scoped to its genesis hash, for
	/// the native allowance paths that key the chain-context cache by it.
	pub async fn chain_client(
		&self,
		label: &'static str,
	) -> Result<crate::runtime::statement_allowance::ChainClient, StatementStoreRpcClientError> {
		Ok(crate::runtime::statement_allowance::ChainClient::new(
			crate::runtime::statement_allowance::rpc::RpcClient::new(RpcClient::new(
				self.request_client(label).await?,
			)),
			self.people_chain_genesis_hash,
		))
	}

	/// Open a statement-store RPC client over its own host-provided
	/// People-chain connection, for statement subscriptions.
	pub async fn client(
		&self,
		label: &'static str,
	) -> Result<RpcClient, StatementStoreRpcClientError> {
		let connection = self.connect(label).await?;
		Ok(RpcClient::new(HostRpcClient::new(connection, self.spawner.clone())))
	}

	/// Submit a SCALE-encoded statement and wait for the JSON-RPC ack.
	pub async fn submit(&self, statement: Vec<u8>, label: &'static str) -> Result<(), String> {
		let rpc_client = self.request_client(label).await.map_err(|err| err.to_string())?;
		submit(&RpcClient::new(rpc_client), statement).await
	}

	/// Submit an SSO statement, tolerating the short propagation window after
	/// an allowance registration is included but not yet visible to the
	/// Statement Store RPC backend.
	pub async fn submit_sso(&self, statement: Vec<u8>, label: &'static str) -> Result<(), String> {
		retry_until_allowance_visible(label, || self.submit(statement.clone(), label)).await
	}

	/// Submit a SCALE-encoded statement without waiting for the JSON-RPC ack.
	pub async fn submit_fire_and_forget(
		&self,
		statement: Vec<u8>,
		label: &'static str,
	) -> Result<(), String> {
		let rpc_client = self.request_client(label).await.map_err(|err| err.to_string())?;
		rpc_client
			.send_fire_and_forget(
				SUBMIT_STATEMENT_METHOD,
				rpc_params![format!("0x{}", hex::encode(&statement))].build(),
			)
			.map_err(rpc_error_message)
	}

	async fn request_client(
		&self,
		label: &'static str,
	) -> Result<HostRpcClient, StatementStoreRpcClientError> {
		Ok(self.chain.rpc_client(label, &self.people_chain_genesis_hash).await?)
	}

	async fn connect(
		&self,
		label: &'static str,
	) -> Result<Arc<dyn JsonRpcConnection>, StatementStoreRpcClientError> {
		self.platform
			.connect(self.people_chain_genesis_hash)
			.await
			.map(Arc::from)
			.map_err(|error| StatementStoreRpcClientError::Connect { label, error })
	}
}

#[async_trait::async_trait]
impl StatementStoreApi for StatementStoreRpc {
	async fn start_statement_watch(
		&self,
		filter: TopicFilter,
	) -> Result<StatementBatches, StatementWatchError> {
		let (kind, topics) =
			validated_topic_filter(filter).map_err(StatementWatchError::NotStarted)?;
		let rpc_client = self
			.client("statement-watch")
			.await
			.map_err(|error| StatementWatchError::NotStarted(error.to_string()))?;
		start_statement_watch_over(&rpc_client, kind, &topics).await
	}
}

/// Starts watching the statements `kind` and `topics` match over
/// `rpc_client`, as [`StatementStoreApi::start_statement_watch`] describes.
async fn start_statement_watch_over(
	rpc_client: &RpcClient,
	kind: TopicFilterKind,
	topics: &[[u8; 32]],
) -> Result<StatementBatches, StatementWatchError> {
	let subscription = subscribe(rpc_client, kind, topics)
		.await
		.map_err(|error| StatementWatchError::NotStarted(rpc_error_message(error)))?;
	let subscription_id = subscription
		.subscription_id()
		.map(ToString::to_string)
		.ok_or_else(|| StatementWatchError::NotStarted("the subscription has no id".to_owned()))?;
	let pages =
		subscription.map(move |notification| parse_statement_page(&subscription_id, notification));
	Ok(backlog_then_live_batches(pages).take_until_inclusive(Result::is_err))
}

/// Joins every page up to and including the first with `remaining == Some(0)`
/// into one batch, then passes each later page on as its own batch. An `Err`
/// passes on as it is.
fn backlog_then_live_batches<E: Send + 'static>(
	pages: impl Stream<Item = Result<NewStatements, E>> + Send + 'static,
) -> BoxStream<'static, Result<Vec<Vec<u8>>, E>> {
	let state = SubscriptionState::CollectingInitial(Vec::new());
	let batching = BatchedPages { pages: pages.boxed(), state };
	stream::unfold(batching, BatchedPages::next_batch).boxed()
}

/// Where a subscription is in delivering its statements.
enum SubscriptionState {
	/// Collecting the statements already stored, joined so far.
	CollectingInitial(Vec<Vec<u8>>),
	/// The stored statements are delivered; each page is new statements.
	WatchingUpdates,
}

/// A subscription's pages, and where it is in delivering them.
struct BatchedPages<E> {
	pages: BoxStream<'static, Result<NewStatements, E>>,
	state: SubscriptionState,
}

/// The state after one page, and the batch that page completed, if any.
struct PageOutcome<E> {
	state: SubscriptionState,
	batch: Option<Result<Vec<Vec<u8>>, E>>,
}

impl<E> BatchedPages<E> {
	/// The next batch, with the pages that yield the ones after it, or `None`
	/// once the pages end.
	async fn next_batch(mut self) -> Option<(Result<Vec<Vec<u8>>, E>, Self)> {
		loop {
			let page = self.pages.next().await?;
			let state = core::mem::replace(&mut self.state, SubscriptionState::WatchingUpdates);
			let outcome = take_page(state, page);
			self.state = outcome.state;
			if let Some(batch) = outcome.batch {
				return Some((batch, self));
			}
		}
	}
}

/// Adds `page` to the initial statements while they are being collected, and
/// makes a batch of them once `page` completes them, or of `page` alone after
/// that.
fn take_page<E>(state: SubscriptionState, page: Result<NewStatements, E>) -> PageOutcome<E> {
	let page = match page {
		Ok(page) => page,
		Err(error) => return PageOutcome { state, batch: Some(Err(error)) },
	};
	let SubscriptionState::CollectingInitial(mut initial) = state else {
		let batch = Some(Ok(page.statements));
		return PageOutcome { state: SubscriptionState::WatchingUpdates, batch };
	};
	initial.extend(page.statements);
	if page.remaining == Some(0) {
		return PageOutcome { state: SubscriptionState::WatchingUpdates, batch: Some(Ok(initial)) };
	}
	PageOutcome { state: SubscriptionState::CollectingInitial(initial), batch: None }
}

/// The page a statement subscription notification carries.
fn parse_statement_page(
	subscription_id: &str,
	notification: Result<Value, subxt_rpcs::Error>,
) -> Result<NewStatements, StatementWatchError> {
	let value = notification
		.map_err(|error| StatementWatchError::Subscription(rpc_error_message(error)))?;
	parse_new_statements_result(subscription_id.to_string(), &value)
		.map_err(StatementWatchError::MalformedPage)
}

/// Subscribe to statements matching the requested topic filter.
pub async fn subscribe(
	rpc_client: &RpcClient,
	kind: TopicFilterKind,
	topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
	rpc_client
		.subscribe::<Value>(
			SUBSCRIBE_STATEMENT_METHOD,
			rpc_params![filter(kind, topics)],
			UNSUBSCRIBE_STATEMENT_METHOD,
		)
		.await
}

/// Subscribe to statements matching every topic.
pub async fn subscribe_match_all(
	rpc_client: &RpcClient,
	topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
	subscribe(rpc_client, TopicFilterKind::MatchAll, topics).await
}

/// Submit a SCALE-encoded statement and confirm the store accepted it.
///
/// `statement_submit` returns an RPC error only for internal failures; a
/// rejected or invalid statement (e.g. `NoAllowance`, `BadProof`) comes back as
/// `Ok(SubmitResult)`. Treat only `new`/`known` as success, so allowance/proof
/// rejections surface instead of being silently dropped.
pub async fn submit(rpc_client: &RpcClient, statement: Vec<u8>) -> Result<(), String> {
	let result = rpc_client
		.request::<Value>(
			SUBMIT_STATEMENT_METHOD,
			rpc_params![format!("0x{}", hex::encode(&statement))],
		)
		.await
		.map_err(rpc_error_message)?;
	match result.get("status").and_then(Value::as_str) {
		Some("new") | Some("known") => Ok(()),
		_ => Err(format!("statement_submit not accepted: {result}")),
	}
}

pub async fn submit_sso(
	rpc_client: &RpcClient,
	statement: Vec<u8>,
	label: &'static str,
) -> Result<(), String> {
	retry_until_allowance_visible(label, || submit(rpc_client, statement.clone())).await
}

/// Run `submit` until it succeeds, fails with anything but `noAllowance`, or
/// runs out of attempts.
async fn retry_until_allowance_visible<SubmitFn, SubmitFuture>(
	label: &'static str,
	submit: SubmitFn,
) -> Result<(), String>
where
	SubmitFn: Fn() -> SubmitFuture,
	SubmitFuture: Future<Output = Result<(), String>>,
{
	for attempt in 1..=SSO_NO_ALLOWANCE_RETRY_ATTEMPTS {
		match submit().await {
			Ok(()) => return Ok(()),
			Err(reason)
				if is_no_allowance_rejection(&reason) &&
					attempt < SSO_NO_ALLOWANCE_RETRY_ATTEMPTS =>
			{
				warn!(
					label,
					attempt,
					max_attempts = SSO_NO_ALLOWANCE_RETRY_ATTEMPTS,
					"SSO allowance not visible yet; retrying statement submission"
				);
				futures_timer::Delay::new(SSO_NO_ALLOWANCE_RETRY_DELAY).await;
			},
			Err(reason) => return Err(reason),
		}
	}
	unreachable!("the bounded SSO submit loop always returns")
}

/// Whether a [`submit`] failure is a `noAllowance` rejection.
pub fn is_no_allowance_rejection(reason: &str) -> bool {
	reason.contains("noAllowance")
}

/// Statement-store topic filter encoded as JSON-RPC params.
pub fn filter(kind: TopicFilterKind, topics: &[[u8; 32]]) -> Value {
	let topics = topics.iter().map(hex_topic).collect::<Vec<_>>();
	match kind {
		TopicFilterKind::MatchAll => json!({ "matchAll": topics }),
		TopicFilterKind::MatchAny => json!({ "matchAny": topics }),
	}
}

/// Human-readable JSON-RPC error message, preserving user error text when
/// provided by the remote endpoint.
pub fn rpc_error_message(error: subxt_rpcs::Error) -> String {
	match error {
		subxt_rpcs::Error::User(error) => error.message,
		other => other.to_string(),
	}
}

#[cfg(test)]
mod tests;
