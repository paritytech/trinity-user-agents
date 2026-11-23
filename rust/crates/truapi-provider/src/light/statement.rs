//! Allowance admission before smoldot queues a statement for gossip.
//!
//! Storage reads are pinned to smoldot's verified finalized head. A missing
//! proof or deadline is uncertainty, never evidence of an absent allowance.

use std::collections::HashMap;

use futures::channel::oneshot;
use serde_json::{Value, json};
use smoldot::{
	json_rpc::methods::{MethodCall, parse_jsonrpc_client_to_server},
	network::codec::{Proof, decode_statement, statement_hash},
};

const CLIENT_ID_PREFIX: &str = "client:";
const CHECK_ID_PREFIX: &str = "allowance:";

/// A frame to send to smoldot or return to the consumer.
pub enum Action {
	/// Continue a private check or hand a permitted request to smoldot.
	Forward(String),
	/// Deliver a final outcome to the consumer.
	Reply(String),
	/// Discard a late private reply whose deadline already elapsed.
	Ignore,
}

struct Check {
	request: Value,
	key: Option<String>,
	data_len: usize,
	_cancel: oneshot::Sender<()>,
}

/// Pending submissions, bounded by the connection's undelivered-frame budget.
#[derive(Default)]
pub struct Admission {
	next_id: u64,
	pending: HashMap<u64, Check>,
}

impl Admission {
	/// Namespace consumer ids so private storage replies cannot collide with them.
	/// A new check also supplies a cancellation receiver for its deadline.
	pub fn request(
		&mut self,
		request: String,
		ready: bool,
	) -> (Action, Option<(u64, oneshot::Receiver<()>)>) {
		let Ok(mut value) = serde_json::from_str::<Value>(&request) else {
			return (Action::Forward(request), None);
		};
		let escaped = value["id"]
			.as_str()
			.is_some_and(|id| id.starts_with(CLIENT_ID_PREFIX) || id.starts_with(CHECK_ID_PREFIX));
		if escaped {
			value["id"] = Value::String(format!("{CLIENT_ID_PREFIX}{}", value["id"]));
		}
		let request = if escaped { value.to_string() } else { request };
		if value["method"] != "statement_submit" {
			return (Action::Forward(request), None);
		}
		let Ok((_, MethodCall::statement_submit { encoded })) =
			parse_jsonrpc_client_to_server(&request)
		else {
			return (Action::Forward(request), None);
		};
		let Ok(statement) = decode_statement(&encoded.0) else {
			return (Action::Forward(request), None);
		};
		let signer = match statement.proof {
			Some(Proof::Sr25519 { signer, .. } | Proof::Ed25519 { signer, .. }) => signer,
			Some(Proof::Secp256k1Ecdsa { signer, .. }) => statement_hash(&signer),
			None => return (Action::Forward(request), None),
		};
		if !ready {
			return (reply(&value, unknown()), None);
		}
		let id = self.next_id;
		self.next_id += 1;
		let (cancel, cancelled) = oneshot::channel();
		let key = format!("0x{}{}", hex::encode(b":statement_allowance:"), hex::encode(signer));
		self.pending.insert(
			id,
			Check {
				request: value,
				key: Some(key),
				data_len: statement.data.as_ref().map_or(0, Vec::len),
				_cancel: cancel,
			},
		);
		(Action::Forward(rpc(id, "chain_getFinalizedHead", json!([]))), Some((id, cancelled)))
	}

	/// Consume a private storage reply or restore a consumer's original id.
	pub fn response(&mut self, frame: String) -> Action {
		let Ok(mut value) = serde_json::from_str::<Value>(&frame) else {
			return Action::Reply(frame);
		};
		let Some(id) = value["id"].as_str() else {
			return Action::Reply(frame);
		};
		if let Some(original) = id.strip_prefix(CLIENT_ID_PREFIX) {
			if let Ok(original) = serde_json::from_str(original) {
				value["id"] = original;
			}
			return Action::Reply(value.to_string());
		}
		let Some(id) = id.strip_prefix(CHECK_ID_PREFIX).and_then(|id| id.parse::<u64>().ok())
		else {
			return Action::Reply(frame);
		};
		let Some(mut check) = self.pending.remove(&id) else {
			return Action::Ignore;
		};
		if let Some(key) = check.key.take() {
			let Some(hash) = value.get("result").and_then(Value::as_str).filter(|hash| {
				hash.strip_prefix("0x").is_some_and(|hex| {
					hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
				})
			}) else {
				return reply(&check.request, unknown());
			};
			let request = rpc(id, "state_getStorage", json!([key, hash]));
			self.pending.insert(id, check);
			return Action::Forward(request);
		}
		match allowance(value.get("result"), check.data_len) {
			Ok(()) => Action::Forward(check.request.to_string()),
			Err(result) => reply(&check.request, result),
		}
	}

	/// Fail a check without broadcasting when verified storage did not answer.
	pub fn expire(&mut self, id: u64) -> Action {
		match self.pending.remove(&id) {
			Some(check) => reply(&check.request, unknown()),
			None => Action::Ignore,
		}
	}
}

fn rpc(id: u64, method: &str, params: Value) -> String {
	json!({"jsonrpc": "2.0", "id": format!("{CHECK_ID_PREFIX}{id}"), "method": method, "params": params}).to_string()
}

fn unknown() -> Value {
	json!({"status": "unknown", "reason": "allowanceUnavailable"})
}

fn reply(request: &Value, result: Value) -> Action {
	let Some(id) = request.get("id") else {
		return Action::Ignore;
	};
	let original = id
		.as_str()
		.and_then(|id| id.strip_prefix(CLIENT_ID_PREFIX))
		.and_then(|id| serde_json::from_str::<Value>(id).ok());
	Action::Reply(
		json!({"jsonrpc": "2.0", "id": original.as_ref().unwrap_or(id), "result": result})
			.to_string(),
	)
}

fn allowance(result: Option<&Value>, data_len: usize) -> Result<(), Value> {
	let no_allowance = || json!({"status": "rejected", "reason": "noAllowance"});
	let Some(result) = result else {
		return Err(unknown());
	};
	if result.is_null() {
		return Err(no_allowance());
	}
	let mut bytes = [0u8; 8];
	let Some(encoded) = result.as_str().and_then(|s| s.strip_prefix("0x")) else {
		return Err(unknown());
	};
	if hex::decode_to_slice(encoded, &mut bytes).is_err() {
		return Err(unknown());
	}
	let max_count = u32::from_le_bytes(bytes[..4].try_into().expect("four bytes"));
	let max_size = u32::from_le_bytes(bytes[4..].try_into().expect("four bytes"));
	if max_count == 0 || max_size == 0 {
		return Err(no_allowance());
	}
	if u64::try_from(data_len).expect("statement length fits u64") > u64::from(max_size) {
		return Err(
			json!({"status": "rejected", "reason": "dataTooLarge", "submitted_size": data_len, "available_size": max_size}),
		);
	}
	Ok(())
}

#[cfg(test)]
mod tests;
