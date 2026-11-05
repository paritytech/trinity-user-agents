use serde_json::json;

use super::{ExtrinsicStatus, HostRpcClient, RpcClient, extrinsic_status, testing::ScriptedRpc};

#[test]
fn in_block_status_completes_submission() {
	let status = extrinsic_status(&json!({"inBlock": "0x1234"}));

	assert!(matches!(status, ExtrinsicStatus::Included(hash) if hash == "0x1234"));
}

#[test]
fn finalized_status_completes_submission() {
	let status = extrinsic_status(&json!({"finalized": "0xabcd"}));

	assert!(matches!(status, ExtrinsicStatus::Included(hash) if hash == "0xabcd"));
}

#[test]
fn terminal_pool_statuses_reject_the_submission() {
	let statuses: Vec<ExtrinsicStatus> =
		["invalid", "dropped", "usurped", "retracted", "finalityTimeout"]
			.into_iter()
			.map(|key| extrinsic_status(&json!({key: "0x1234"})))
			.collect();

	assert_eq!(
		statuses,
		vec![
			ExtrinsicStatus::Rejected("invalid".to_string()),
			ExtrinsicStatus::Rejected("dropped".to_string()),
			ExtrinsicStatus::Rejected("usurped".to_string()),
			ExtrinsicStatus::Rejected("retracted".to_string()),
			ExtrinsicStatus::Rejected("finalityTimeout".to_string()),
		],
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
