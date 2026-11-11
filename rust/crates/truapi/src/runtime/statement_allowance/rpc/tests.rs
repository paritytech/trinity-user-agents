use serde_json::{Value, json};

use super::{
	ExtrinsicStatus, HostRpcClient, RpcClient, RpcError, StatementAllowanceError, extrinsic_status,
	testing::ScriptedRpc,
};

/// What a status notification, as the pool sends it, means for the submission.
fn status(notification: Value) -> ExtrinsicStatus {
	extrinsic_status(serde_json::from_value(notification).expect("a pool status"))
}

#[test]
fn in_block_status_completes_submission() {
	assert_eq!(
		status(json!({"inBlock": "0x1234"})),
		ExtrinsicStatus::Included("0x1234".to_string())
	);
}

#[test]
fn finalized_status_completes_submission() {
	assert_eq!(
		status(json!({"finalized": "0xabcd"})),
		ExtrinsicStatus::Included("0xabcd".to_string())
	);
}

/// Each status in the form the pool sends it. A rejection that is not
/// recognised waits out the whole submit timeout before it surfaces.
#[test]
fn terminal_pool_statuses_reject_the_submission() {
	let statuses: Vec<ExtrinsicStatus> = [
		json!("invalid"),
		json!("dropped"),
		json!({"usurped": "0x1234"}),
		json!({"retracted": "0x1234"}),
		json!({"finalityTimeout": "0x1234"}),
		json!("ready"),
		json!({"broadcast": ["12D3KooW"]}),
	]
	.into_iter()
	.map(status)
	.collect();

	assert_eq!(
		statuses,
		vec![
			ExtrinsicStatus::Rejected("invalid"),
			ExtrinsicStatus::Rejected("dropped"),
			ExtrinsicStatus::Rejected("usurped"),
			ExtrinsicStatus::Rejected("retracted"),
			ExtrinsicStatus::Rejected("finalityTimeout"),
			ExtrinsicStatus::Pending,
			ExtrinsicStatus::Pending,
		],
	);
}

/// A rejection the pool sends as a bare string ends the submission there,
/// rather than being taken for progress until the subscription or the
/// submit timeout runs out.
#[test]
fn a_dropped_submission_fails_at_once() {
	let scripted = ScriptedRpc::new([]);
	scripted.script_subscription([r#""ready""#, r#""dropped""#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let Err(error) = futures::executor::block_on(rpc.submit_and_watch(&[0x01])) else {
		panic!("the pool dropped the transaction");
	};
	assert!(
		matches!(
			&error,
			StatementAllowanceError::Rpc(RpcError::ExtrinsicRejected { status })
				if status == "dropped"
		),
		"{error}"
	);
}

#[test]
fn get_storage_at_pins_the_read_to_a_block() {
	let scripted = ScriptedRpc::new([r#""0x0102""#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let value = futures::executor::block_on(rpc.get_storage_at(b"key", "0xat")).unwrap();

	assert_eq!(value, Some(vec![0x01, 0x02]));
	assert_eq!(
		scripted.calls(),
		vec![("state_getStorage".to_string(), r#"["0x6b6579","0xat"]"#.to_string(),)],
	);
}

#[test]
fn get_storage_reads_at_the_current_block() {
	let scripted = ScriptedRpc::new(["null"]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let value = futures::executor::block_on(rpc.get_storage(b"key")).unwrap();

	assert_eq!(value, None);
	assert_eq!(
		scripted.calls(),
		vec![("state_getStorage".to_string(), r#"["0x6b6579"]"#.to_string())],
	);
}

#[test]
fn finalized_head_returns_the_hash() {
	let scripted = ScriptedRpc::new([r#""0xfeed""#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let head = futures::executor::block_on(rpc.finalized_head()).unwrap();

	assert_eq!(head, "0xfeed");
	assert_eq!(scripted.calls(), vec![("chain_getFinalizedHead".to_string(), "[]".to_string())],);
}

#[test]
fn inspection_batches_require_complete_unambiguous_pinned_values() {
	let keys = vec!["0xaa".to_string(), "0xbb".to_string()];
	let valid = json!([{"block": "0xat", "changes": [["0xbb", null], ["0xaa", "0x01"]]}]);
	assert_eq!(
		super::decode_storage_batch_at(&keys, "0xat", valid).unwrap(),
		vec![Some(vec![1]), None],
	);
	for invalid in [
		json!([]),
		json!([{"block": "0xother", "changes": [["0xaa", null], ["0xbb", null]]}]),
		json!([{"block": "0xat", "changes": [["0xaa", null]]}]),
		json!([{"block": "0xat", "changes": [["0xaa", null], ["0xaa", null]]}]),
		json!([{"block": "0xat", "changes": [["0xaa", null], ["0xcc", null]]}]),
		json!([{"block": "0xat", "changes": [["0xaa", 0], ["0xbb", null]]}]),
	] {
		assert!(super::decode_storage_batch_at(&keys, "0xat", invalid).is_err());
	}
}
