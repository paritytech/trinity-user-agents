//! The json-rpc client of one chain connection, which answers the runtime calls
//! every client of the chain makes alike from the shared [`RuntimeCallCache`].
//!
//! Product follows and the Subxt clients of the core send their requests
//! through it, so they share one download of the metadata of each runtime. A
//! call the cache answers gets an operation id from the cache, and its outcome
//! arrives on the follow it was made on, the way the node would send it.

use std::{
	collections::HashMap,
	pin::Pin,
	sync::{Arc, Mutex},
	task::{Context, Poll},
};

use futures::{Stream, channel::mpsc};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use subxt_rpcs::{
	Error as RpcError,
	client::{RawRpcFuture, RawRpcSubscription, RpcClientT},
};

use super::runtime_call_cache::{
	Answer, AnswerSender, Claim, RuntimeCall, RuntimeCallCache, is_cached_operation,
};
use crate::host_rpc_client::HostRpcClient;

/// The storage key the runtime code lives under, `:code`.
const CODE_KEY: &str = "0x3a636f6465";

type EventStream = Pin<Box<dyn Stream<Item = Result<Box<RawValue>, RpcError>> + Send + 'static>>;

/// One chain connection, with the runtime calls that depend only on the
/// runtime answered from the cache every connection to the chain shares.
#[derive(Clone)]
pub struct RuntimeCallClient {
	node: HostRpcClient,
	genesis_hash: Vec<u8>,
	cache: RuntimeCallCache,
	/// Whether a call waits for a download another caller has in flight. Only a
	/// client that retries an inaccessible operation may wait, since that is
	/// what a waiter hears when the download fails.
	waits_for_downloads: bool,
	follows: Arc<Mutex<HashMap<String, Follow>>>,
}

/// What the client tracks for one chainHead follow on its connection.
struct Follow {
	answers: AnswerSender,
	/// Downloads made on this follow, by the operation id the node gave them,
	/// with the cache fetch they stand for when callers wait on them.
	downloads: HashMap<String, (RuntimeCall, Option<u64>)>,
	/// Downloads whose reply from the node is still on its way.
	replies_pending: usize,
	/// Operation ends the node reported while a download reply was on its way,
	/// since the end can arrive before the reply that names the operation.
	early_ends: HashMap<String, Box<RawValue>>,
	/// The hash of the runtime code at each block a call asked about.
	code_hashes: HashMap<String, Vec<u8>>,
}

impl Follow {
	fn new(answers: AnswerSender) -> Self {
		Self {
			answers,
			downloads: HashMap::new(),
			replies_pending: 0,
			early_ends: HashMap::new(),
			code_hashes: HashMap::new(),
		}
	}
}

/// The fields of a follow event that say which operation it ends.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationEnd {
	event: String,
	#[serde(default)]
	operation_id: Option<String>,
}

/// The output a follow event carries, when it reports a call done.
#[derive(Deserialize)]
struct CallOutput {
	event: String,
	#[serde(default)]
	output: Option<String>,
}

/// The reply to `chainHead_v1_call`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallReply {
	result: String,
	#[serde(default)]
	operation_id: Option<String>,
}

/// A download claimed from the cache while its reply from the node is on its way.
///
/// Dropped before the reply is recorded, as when the caller goes away, it gives
/// the download up, so the callers waiting on it are told to ask again.
struct ClaimedDownload<'a> {
	client: &'a RuntimeCallClient,
	follow: &'a str,
	call: &'a RuntimeCall,
	fetch: Option<u64>,
	settled: bool,
}

impl Drop for ClaimedDownload<'_> {
	fn drop(&mut self) {
		if self.settled {
			return;
		}
		self.client.with_follow(self.follow, |state| state.replies_pending -= 1);
		self.client.give_up(self.call, self.fetch);
	}
}

impl RuntimeCallClient {
	/// The client of the connection `node` to the chain `genesis_hash`, for
	/// callers that retry an inaccessible operation.
	pub fn new(node: HostRpcClient, genesis_hash: Vec<u8>, cache: RuntimeCallCache) -> Self {
		Self {
			node,
			genesis_hash,
			cache,
			waits_for_downloads: true,
			follows: Arc::new(Mutex::new(HashMap::new())),
		}
	}

	/// The same connection for a caller that never retries an inaccessible
	/// operation: it reuses finished downloads and makes its own otherwise.
	pub fn without_waiting(&self) -> Self {
		Self { waits_for_downloads: false, ..self.clone() }
	}

	async fn call(&self, params: Option<Box<RawValue>>) -> Result<Box<RawValue>, RpcError> {
		let Some((follow, call, answers)) = self.shared_call(params.as_deref()).await else {
			return self.node.request_raw("chainHead_v1_call", params).await;
		};
		let fetch = match self.cache.claim(&call, &answers, self.waits_for_downloads) {
			Claim::Answered { operation_id } | Claim::Waiting { operation_id } => {
				return raw(&json!({"result": "started", "operationId": operation_id}));
			},
			Claim::Fetch { fetch } => Some(fetch),
			Claim::FetchAlongside => None,
		};
		if self.with_follow(&follow, |state| state.replies_pending += 1).is_none() {
			self.give_up(&call, fetch);
			return self.node.request_raw("chainHead_v1_call", params).await;
		}
		let mut claimed =
			ClaimedDownload { client: self, follow: &follow, call: &call, fetch, settled: false };
		let reply = self.node.request_raw("chainHead_v1_call", params).await;
		let operation_id = reply
			.as_ref()
			.ok()
			.and_then(|reply| serde_json::from_str::<CallReply>(reply.get()).ok())
			.filter(|reply| reply.result == "started")
			.and_then(|reply| reply.operation_id);
		let Some(early_end) = self.with_follow(&follow, |state| {
			claimed.settled = true;
			state.replies_pending -= 1;
			let early_end = operation_id.as_ref().and_then(|id| state.early_ends.remove(id));
			if let (Some(id), None) = (&operation_id, &early_end) {
				state.downloads.insert(id.clone(), (call.clone(), fetch));
			}
			if state.replies_pending == 0 {
				state.early_ends.clear();
			}
			early_end
		}) else {
			return reply;
		};
		match (operation_id, early_end) {
			(Some(_), Some(end)) => self.settle(&call, fetch, &end),
			(Some(_), None) => {},
			(None, _) => self.give_up(&call, fetch),
		}
		reply
	}

	/// The follow a `chainHead_v1_call` with `params` was made on, its shared
	/// form and where to answer it, when its output depends only on the runtime
	/// code of its block and the hash of that code can be read.
	async fn shared_call(
		&self,
		params: Option<&RawValue>,
	) -> Option<(String, RuntimeCall, AnswerSender)> {
		let (follow, block, function, call_parameters): (String, String, String, String) =
			serde_json::from_str(params?.get()).ok()?;
		if !RuntimeCall::depends_only_on_runtime(&function) {
			return None;
		}
		let answers = self.with_follow(&follow, |state| state.answers.clone())?;
		let code_hash = self.code_hash(&follow, &block).await?;
		let call_parameters = hex::decode(call_parameters.trim_start_matches("0x")).ok()?;
		let call = RuntimeCall::new(&self.genesis_hash, code_hash, function, call_parameters);
		Some((follow, call, answers))
	}

	/// The hash of the runtime code at `block`, asked of the node directly so the
	/// read leaves no operation behind on the follow.
	async fn code_hash(&self, follow: &str, block: &str) -> Option<Vec<u8>> {
		let known = self
			.with_follow(follow, |state| state.code_hashes.get(block).cloned())
			.flatten();
		if known.is_some() {
			return known;
		}
		let params = raw(&json!([CODE_KEY, block])).ok()?;
		let reply = self.node.request_raw("state_getStorageHash", Some(params)).await.ok()?;
		let code_hash: Option<String> = serde_json::from_str(reply.get()).ok()?;
		let code_hash = hex::decode(code_hash?.trim_start_matches("0x")).ok()?;
		self.with_follow(follow, |state| {
			state.code_hashes.insert(block.to_string(), code_hash.clone())
		});
		Some(code_hash)
	}

	async fn stop_operation(
		&self,
		params: Option<Box<RawValue>>,
	) -> Result<Box<RawValue>, RpcError> {
		let target: Option<(String, String)> =
			params.as_deref().and_then(|params| serde_json::from_str(params.get()).ok());
		let Some((follow, operation_id)) = target else {
			return self.node.request_raw("chainHead_v1_stopOperation", params).await;
		};
		if is_cached_operation(&operation_id) {
			self.cache.release(&operation_id);
			return raw(&Value::Null);
		}
		let download = self
			.with_follow(&follow, |state| state.downloads.remove(&operation_id))
			.flatten();
		let reply = self.node.request_raw("chainHead_v1_stopOperation", params).await;
		if let Some((call, fetch)) = download {
			self.give_up(&call, fetch);
		}
		reply
	}

	async fn follow(
		&self,
		sub: &str,
		params: Option<Box<RawValue>>,
		unsub: &str,
	) -> Result<RawRpcSubscription, RpcError> {
		let subscription = self.node.subscribe_raw(sub, params, unsub).await?;
		let Some(id) = subscription.id.clone() else {
			return Ok(subscription);
		};
		let (answers, cached) = mpsc::unbounded();
		self.follows.lock().unwrap().insert(id.clone(), Follow::new(answers));
		let events = FollowEvents {
			node: subscription.stream,
			cached,
			client: self.clone(),
			subscription: id.clone(),
		};
		Ok(RawRpcSubscription { stream: Box::pin(events), id: Some(id) })
	}

	/// Settle the download an event of the follow `subscription` ends, if any.
	fn observe(&self, subscription: &str, event: &RawValue) {
		let Ok(OperationEnd { event: kind, operation_id: Some(operation_id) }) =
			serde_json::from_str::<OperationEnd>(event.get())
		else {
			return;
		};
		if !matches!(
			kind.as_str(),
			"operationCallDone" | "operationError" | "operationInaccessible"
		) {
			return;
		}
		let download = self
			.with_follow(subscription, |state| {
				let download = state.downloads.remove(&operation_id);
				if download.is_none() && state.replies_pending > 0 {
					state.early_ends.insert(operation_id.clone(), event.to_owned());
				}
				download
			})
			.flatten();
		if let Some((call, fetch)) = download {
			self.settle(&call, fetch, event);
		}
	}

	/// Share the output of a download the node finished, or give up on it.
	fn settle(&self, call: &RuntimeCall, fetch: Option<u64>, end: &RawValue) {
		let output = serde_json::from_str::<CallOutput>(end.get())
			.ok()
			.filter(|end| end.event == "operationCallDone")
			.and_then(|end| end.output)
			.and_then(|output| hex::decode(output.trim_start_matches("0x")).ok());
		match output {
			Some(output) => self.cache.fulfil(call, output),
			None => self.give_up(call, fetch),
		}
	}

	fn give_up(&self, call: &RuntimeCall, fetch: Option<u64>) {
		if let Some(fetch) = fetch {
			self.cache.abandon(call, fetch);
		}
	}

	/// Drop the follow `subscription`, whose downloads will no longer end.
	fn forget(&self, subscription: &str) {
		let Some(state) = self.follows.lock().unwrap().remove(subscription) else {
			return;
		};
		for (call, fetch) in state.downloads.into_values() {
			self.give_up(&call, fetch);
		}
	}

	fn with_follow<T>(&self, follow: &str, apply: impl FnOnce(&mut Follow) -> T) -> Option<T> {
		self.follows.lock().unwrap().get_mut(follow).map(apply)
	}
}

impl RpcClientT for RuntimeCallClient {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		Box::pin(async move {
			match method {
				"chainHead_v1_call" => self.call(params).await,
				"chainHead_v1_stopOperation" => self.stop_operation(params).await,
				_ => self.node.request_raw(method, params).await,
			}
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<RawValue>>,
		unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		Box::pin(async move {
			if sub == "chainHead_v1_follow" {
				self.follow(sub, params, unsub).await
			} else {
				self.node.subscribe_raw(sub, params, unsub).await
			}
		})
	}
}

/// The events of one follow: the node events, and the outcomes of the calls
/// the cache answers on it.
struct FollowEvents {
	node: EventStream,
	cached: mpsc::UnboundedReceiver<Answer>,
	client: RuntimeCallClient,
	subscription: String,
}

impl Stream for FollowEvents {
	type Item = Result<Box<RawValue>, RpcError>;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = self.get_mut();
		if let Poll::Ready(Some(answer)) = Pin::new(&mut this.cached).poll_next(cx) {
			return Poll::Ready(Some(answer_event(&answer)));
		}
		let next = this.node.as_mut().poll_next(cx);
		if let Poll::Ready(Some(Ok(event))) = &next {
			this.client.observe(&this.subscription, event);
		}
		next
	}
}

impl Drop for FollowEvents {
	fn drop(&mut self) {
		self.client.forget(&self.subscription);
	}
}

/// The follow event the node would send for `answer`.
fn answer_event(answer: &Answer) -> Result<Box<RawValue>, RpcError> {
	match answer {
		Answer::Done { operation_id, output } => raw(&json!({
			"event": "operationCallDone",
			"operationId": operation_id,
			"output": format!("0x{}", hex::encode(output.as_slice())),
		})),
		Answer::Inaccessible { operation_id } => {
			raw(&json!({"event": "operationInaccessible", "operationId": operation_id}))
		},
	}
}

fn raw(value: &Value) -> Result<Box<RawValue>, RpcError> {
	RawValue::from_string(value.to_string()).map_err(RpcError::Deserialization)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use core::time::Duration;
	use std::sync::atomic::{AtomicUsize, Ordering};

	use futures::{
		FutureExt, StreamExt,
		future::{AbortHandle, Abortable},
		pin_mut,
	};

	use super::*;
	use crate::{
		chain_runtime::RuntimeChainProvider,
		subscription::thread_per_subscription_spawner,
		test_support::{ScriptedProvider, extract_id, notification_sender, wait_for_sent},
	};

	const FOLLOW: &str = "FOLLOW";

	/// A node whose follows are all `FOLLOW`, whose blocks share one runtime, and
	/// which leaves the first `chainHead_v1_call` unanswered and starts the rest.
	fn node() -> ScriptedProvider {
		let calls = AtomicUsize::new(0);
		ScriptedProvider::with_frames(move |request| {
			let id = extract_id(request).unwrap();
			let result = match serde_json::from_str::<Value>(request).unwrap()["method"].as_str() {
				Some("chainHead_v1_follow") => json!(FOLLOW),
				Some("state_getStorageHash") => json!(format!("0x{}", hex::encode([0xc1; 32]))),
				Some("chainHead_v1_call") if calls.fetch_add(1, Ordering::SeqCst) > 0 => {
					json!({"result": "started", "operationId": "download"})
				},
				_ => return Vec::new(),
			};
			vec![json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()]
		})
	}

	fn metadata_call() -> Option<Box<RawValue>> {
		let block = format!("0x{}", hex::encode([0x11; 32]));
		raw(&json!([FOLLOW, block, "Metadata_metadata_at_version", "0x0f000000"])).ok()
	}

	fn within_five_seconds<T>(future: impl Future<Output = T>) -> Option<T> {
		futures::executor::block_on(async {
			let future = future.fuse();
			let deadline = futures_timer::Delay::new(Duration::from_secs(5)).fuse();
			pin_mut!(future, deadline);
			futures::select! {
				output = future => Some(output),
				() = deadline => None,
			}
		})
	}

	/// The node can end other operations on a follow while a download reply is
	/// on its way. Those ends are kept only until no reply is left, counting a
	/// reply whose caller went away, so a follow does not collect them for as
	/// long as it lives.
	#[test]
	fn a_follow_keeps_no_operation_ends_once_no_download_reply_is_pending() {
		let provider = node();
		let connection = futures::executor::block_on(provider.connect(vec![0xab; 32])).unwrap();
		let client = RuntimeCallClient::new(
			HostRpcClient::new(connection, thread_per_subscription_spawner()),
			vec![0xab; 32],
			RuntimeCallCache::default(),
		);
		let follow_params = raw(&json!([true])).ok();
		let mut follow = futures::executor::block_on(client.subscribe_raw(
			"chainHead_v1_follow",
			follow_params,
			"chainHead_v1_unfollow",
		))
		.unwrap();
		let caller = client.clone();
		let (abort, registration) = AbortHandle::new_pair();
		let dropped_call = std::thread::spawn(move || {
			let call = caller.request_raw("chainHead_v1_call", metadata_call());
			futures::executor::block_on(Abortable::new(call, registration))
		});
		wait_for_sent(&provider, |sent| {
			sent.iter().any(|request| request.contains("chainHead_v1_call"))
		});
		let other_operation_done = json!({
			"jsonrpc": "2.0",
			"method": "chainHead_v1_followEvent",
			"params": {"subscription": FOLLOW, "result": {
				"event": "operationCallDone",
				"operationId": "other",
				"output": "0x00",
			}},
		});
		notification_sender(&provider)
			.unbounded_send(other_operation_done.to_string())
			.unwrap();
		within_five_seconds(follow.stream.next()).expect("the follow reports the other operation");
		abort.abort();
		for _ in 0..500 {
			if dropped_call.is_finished() {
				break;
			}
			std::thread::sleep(Duration::from_millis(10));
		}

		within_five_seconds(client.request_raw("chainHead_v1_call", metadata_call()))
			.expect("the node starts the download")
			.unwrap();

		assert_eq!(client.with_follow(FOLLOW, |state| state.early_ends.len()), Some(0));
	}
}
