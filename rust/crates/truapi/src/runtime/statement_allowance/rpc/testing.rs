//! Scripted JSON-RPC transport for exercising request shapes in tests.

use std::sync::{Arc, Mutex};

use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClientT};

/// Records every request as `(method, params)` and replays canned JSON
/// results in order; subscriptions replay the scripted notification items.
#[derive(Clone, Default)]
pub struct ScriptedRpc(Arc<Inner>);

#[derive(Default)]
struct Inner {
	calls: Mutex<Vec<(String, String)>>,
	responses: Mutex<Vec<String>>,
	subscription_batches: Mutex<Vec<Vec<String>>>,
	subscription_errors: Mutex<Vec<String>>,
}

impl ScriptedRpc {
	/// A script answering requests with `responses`, in order.
	pub fn new<'a>(responses: impl IntoIterator<Item = &'a str>) -> Self {
		let scripted = Self::default();
		*scripted.0.responses.lock().unwrap() = responses.into_iter().map(str::to_owned).collect();
		scripted
	}

	/// Queue the notification items for one subscription. Call once per
	/// expected submission; batches are replayed in order.
	pub fn script_subscription<'a>(&self, items: impl IntoIterator<Item = &'a str>) {
		self.0
			.subscription_batches
			.lock()
			.unwrap()
			.push(items.into_iter().map(str::to_owned).collect());
	}

	/// Fail the next `n` subscriptions with `message`, as the node does when
	/// it rejects a submission outright.
	pub fn script_subscription_errors(&self, message: &str, count: usize) {
		*self.0.subscription_errors.lock().unwrap() =
			std::iter::repeat_n(message.to_owned(), count).collect();
	}

	/// The `(method, params)` pairs seen so far.
	pub fn calls(&self) -> Vec<(String, String)> {
		self.0.calls.lock().unwrap().clone()
	}
}

/// Answer a batched storage read with one scripted answer per key, in key
/// order, as if each key had been read on its own.
fn batched_storage_answer(responses: &mut Vec<String>, params: &str) -> String {
	let params: serde_json::Value =
		serde_json::from_str(params).expect("batched read params are JSON");
	let keys = params[0].as_array().expect("batched read names its keys");
	assert!(responses.len() >= keys.len(), "unscripted batched read of {} keys", keys.len());
	let changes: Vec<serde_json::Value> = keys
		.iter()
		.map(|key| {
			let value: serde_json::Value = serde_json::from_str(&responses.remove(0))
				.expect("scripted response is valid JSON");
			serde_json::json!([key, value])
		})
		.collect();
	serde_json::json!([{ "block": "0xscripted", "changes": changes }]).to_string()
}

fn params_json(params: Option<Box<RawValue>>) -> String {
	params.map_or_else(|| "[]".to_string(), |p| p.get().to_owned())
}

impl RpcClientT for ScriptedRpc {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		let params = params_json(params);
		self.0.calls.lock().unwrap().push((method.to_owned(), params.clone()));
		let mut responses = self.0.responses.lock().unwrap();
		assert!(!responses.is_empty(), "unscripted request `{method}`");
		let response = if method == "state_queryStorageAt" {
			batched_storage_answer(&mut responses, &params)
		} else {
			responses.remove(0)
		};
		Box::pin(async move {
			Ok(RawValue::from_string(response).expect("scripted response is valid JSON"))
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<RawValue>>,
		_unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		self.0.calls.lock().unwrap().push((sub.to_owned(), params_json(params)));
		let failure = {
			let mut errors = self.0.subscription_errors.lock().unwrap();
			(!errors.is_empty()).then(|| errors.remove(0))
		};
		if let Some(message) = failure {
			return Box::pin(async move { Err(subxt_rpcs::Error::Client(message.into())) });
		}
		let batch = {
			let mut batches = self.0.subscription_batches.lock().unwrap();
			if batches.is_empty() { Vec::new() } else { batches.remove(0) }
		};
		let items: Vec<_> = batch
			.into_iter()
			.map(|item| Ok(RawValue::from_string(item).expect("scripted item is valid JSON")))
			.collect();
		Box::pin(async move {
			Ok(RawRpcSubscription {
				stream: Box::pin(futures::stream::iter(items)),
				id: Some("scripted".to_string()),
			})
		})
	}
}
