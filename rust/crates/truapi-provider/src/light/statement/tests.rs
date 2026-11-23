use futures::{FutureExt, executor::block_on};
use serde_json::{Value, json};
use smoldot::network::codec::{Proof, Statement, encode_statement, statement_hash};

use super::{Action, Admission};

fn request(proof: Proof, data_len: usize) -> Value {
	let encoded = encode_statement(&Statement {
		proof: Some(proof),
		decryption_key: None,
		expiry: u64::MAX,
		channel: None,
		topics: vec![[3; 32]],
		data: Some(vec![7; data_len]),
	})
	.unwrap();
	json!({"jsonrpc": "2.0", "id": 42, "method": "statement_submit", "params": [format!("0x{}", hex::encode(encoded))]})
}

fn signed_request(data_len: usize) -> Value {
	request(Proof::Sr25519 { signer: [9; 32], signature: [0; 64] }, data_len)
}

fn forwarded(action: Action) -> Value {
	let Action::Forward(frame) = action else { panic!("expected an upstream request") };
	serde_json::from_str(&frame).unwrap()
}

fn replied(action: Action) -> Value {
	let Action::Reply(frame) = action else { panic!("expected a consumer reply, not a broadcast") };
	serde_json::from_str(&frame).unwrap()
}

fn answer(admission: &mut Admission, request: &Value, result: Value) -> Action {
	admission.response(json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string())
}

fn storage_request(
	admission: &mut Admission,
	submit: &Value,
) -> (Value, futures::channel::oneshot::Receiver<()>) {
	let (action, deadline) = admission.request(submit.to_string());
	let head = forwarded(action);
	assert_eq!(
		(head["method"].clone(), head["params"].clone()),
		(json!("chain_getBlockHash"), json!([]))
	);
	let hash = format!("0x{}", "ab".repeat(32));
	let storage = forwarded(answer(admission, &head, json!(hash)));
	assert_eq!(
		(storage["method"].clone(), storage["params"][1].clone()),
		(json!("state_getStorage"), json!(hash))
	);
	(storage, deadline.unwrap().1)
}

fn capacity(count: u32, bytes: u32) -> Value {
	let mut allowance = count.to_le_bytes().to_vec();
	allowance.extend(bytes.to_le_bytes());
	json!(format!("0x{}", hex::encode(allowance)))
}

#[test]
fn absent_or_depleted_allowance_rejects_without_broadcasting() {
	for allowance in [Value::Null, capacity(0, 100), capacity(1, 0)] {
		let mut admission = Admission::default();
		let (storage, cancelled) = storage_request(&mut admission, &signed_request(4));
		assert_eq!(
			replied(answer(&mut admission, &storage, allowance)),
			json!({
				"jsonrpc": "2.0", "id": 42, "result": {"status": "rejected", "reason": "noAllowance"}
			})
		);
		assert!(block_on(cancelled).is_err());
	}
}

#[test]
fn usable_allowance_broadcasts_then_preserves_smoldots_result() {
	let mut admission = Admission::default();
	let submit = signed_request(4);
	let (storage, cancelled) = storage_request(&mut admission, &submit);
	assert_eq!(
		storage["params"][0],
		format!("0x{}{}", hex::encode(b":statement_allowance:"), "09".repeat(32))
	);
	assert_eq!(forwarded(answer(&mut admission, &storage, capacity(1, 4))), submit);
	assert!(block_on(cancelled).is_err());
	assert_eq!(
		replied(answer(&mut admission, &submit, json!({"status": "new"}))),
		json!({
			"jsonrpc": "2.0", "id": 42, "result": {"status": "new"}
		})
	);
}

#[test]
fn allowance_size_counts_data_not_encoded_statement_overhead() {
	let mut admission = Admission::default();
	let submit = signed_request(5);
	let (storage, _cancelled) = storage_request(&mut admission, &submit);
	assert_eq!(
		replied(answer(&mut admission, &storage, capacity(1, 4)))["result"],
		json!({
			"status": "rejected", "reason": "dataTooLarge", "submitted_size": 5, "available_size": 4
		})
	);
}

#[test]
fn unavailable_or_malformed_proofs_are_unknown_not_no_allowance() {
	for response in [
		json!({"error": {"code": -32000, "message": "storage proof unavailable"}}),
		json!({"result": "0x"}),
		json!({"result": "0x01000000040000"}),
		json!({"result": "0x010000000400000000"}),
		json!({"result": 123}),
	] {
		let mut admission = Admission::default();
		let (storage, _cancelled) = storage_request(&mut admission, &signed_request(4));
		let mut response = response;
		response["id"] = storage["id"].clone();
		assert_eq!(
			replied(admission.response(response.to_string()))["result"],
			json!({
				"status": "unknown", "reason": "allowanceUnavailable"
			})
		);
	}
}

#[test]
fn an_unreadable_best_head_does_not_broadcast() {
	let mut admission = Admission::default();
	for hash in [json!(null), json!("0x1234"), json!("not a block hash")] {
		let (action, _deadline) = admission.request(signed_request(4).to_string());
		let head = forwarded(action);
		assert_eq!(
			replied(answer(&mut admission, &head, hash))["result"],
			json!({"status": "unknown", "reason": "allowanceUnavailable"})
		);
	}
}

#[test]
fn a_timed_out_check_ignores_late_proofs_and_can_be_retried() {
	let mut admission = Admission::default();
	let (action, deadline) = admission.request(signed_request(4).to_string());
	let head = forwarded(action);
	let (id, cancelled) = deadline.unwrap();
	assert!(cancelled.now_or_never().is_none());
	assert_eq!(
		replied(admission.expire(id))["result"],
		json!({"status": "unknown", "reason": "allowanceUnavailable"})
	);
	assert!(matches!(
		answer(&mut admission, &head, json!(format!("0x{}", "ab".repeat(32)))),
		Action::Ignore
	));
	let (storage, _cancelled) = storage_request(&mut admission, &signed_request(4));
	assert_eq!(
		forwarded(answer(&mut admission, &storage, capacity(1, 4)))["method"],
		"statement_submit"
	);
}

#[test]
fn signer_account_is_derived_for_each_proof_kind() {
	for (proof, account) in [
		(Proof::Sr25519 { signer: [1; 32], signature: [0; 64] }, [1; 32]),
		(Proof::Ed25519 { signer: [2; 32], signature: [0; 64] }, [2; 32]),
		(Proof::Secp256k1Ecdsa { signer: [3; 33], signature: [0; 65] }, statement_hash(&[3; 33])),
	] {
		let mut admission = Admission::default();
		let (storage, _cancelled) = storage_request(&mut admission, &request(proof, 4));
		assert_eq!(
			storage["params"][0],
			format!("0x{}{}", hex::encode(b":statement_allowance:"), hex::encode(account))
		);
	}
}

#[test]
fn named_params_cannot_bypass_admission() {
	let mut submit = signed_request(4);
	submit["params"] = json!({"encoded": submit["params"][0]});
	let mut admission = Admission::default();
	let (storage, _cancelled) = storage_request(&mut admission, &submit);
	assert_eq!(
		replied(answer(&mut admission, &storage, Value::Null))["result"]["reason"],
		"noAllowance"
	);
}

#[test]
fn private_ids_cannot_collide_with_consumer_requests() {
	let mut admission = Admission::default();
	let (_storage, _cancelled) = storage_request(&mut admission, &signed_request(4));
	for id in [json!("allowance:0"), json!("client:42"), json!(42), Value::Null] {
		let request = json!({"jsonrpc": "2.0", "id": id, "method": "rpc_methods", "params": []});
		let (action, deadline) = admission.request(request.to_string());
		assert!(deadline.is_none());
		let request = forwarded(action);
		assert_eq!(
			replied(answer(&mut admission, &request, json!({"methods": []}))),
			json!({"jsonrpc": "2.0", "id": id, "result": {"methods": []}})
		);
	}
}

#[test]
fn an_allowance_granted_at_best_need_not_be_finalized() {
	let mut admission = Admission::default();
	let submit = signed_request(4);
	let (action, _deadline) = admission.request(submit.to_string());
	let head = forwarded(action);
	let best = format!("0x{}", "ab".repeat(32));
	let finalized = format!("0x{}", "cd".repeat(32));
	let hash = match head["method"].as_str().unwrap() {
		"chain_getBlockHash" => &best,
		"chain_getFinalizedHead" => &finalized,
		other => panic!("unexpected head request: {other}"),
	};
	let storage = forwarded(answer(&mut admission, &head, json!(hash)));
	let allowance = if storage["params"][1] == best { capacity(1, 4) } else { Value::Null };
	assert_eq!(forwarded(answer(&mut admission, &storage, allowance)), submit);
}

#[test]
fn a_new_admission_starts_a_check_without_waiting_for_readiness() {
	let mut admission = Admission::default();
	let (action, deadline) = admission.request(signed_request(4).to_string());
	assert_eq!(forwarded(action)["method"], "chain_getBlockHash");
	assert!(deadline.is_some());
}

#[test]
fn ordinary_response_pages_need_no_decoding() {
	for frame in [
		r#"{"jsonrpc":"2.0","id":42,"result":{"methods":[]}}"#,
		r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"s","result":{"event":"newBlock"}}}"#,
	] {
		assert!(!Admission::needs_inspection(frame));
	}
	for id in ["allowance:0", "client:\"allowance:0\""] {
		let frame = json!({"jsonrpc": "2.0", "id": id, "result": null}).to_string();
		assert!(Admission::needs_inspection(&frame));
	}
}
