use subxt_rpcs::RpcClient as HostRpcClient;

use super::{
	super::{rpc::testing::ScriptedRpc, view_cache::ViewFunctionDef},
	*,
};

const FIXTURE_V16: &[u8] =
	include_bytes!("../../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

fn response(value: Result<Vec<u8>, ViewFunctionDispatchError>) -> String {
	format!("\"0x{}\"", hex::encode(value.encode()))
}

#[test]
fn no_argument_call_uses_the_metadata_id_and_unwraps_the_dispatch_result() {
	let id = [0x42; 32];
	let response = response(Ok(vec![10, 0, 0, 0]));
	let scripted = ScriptedRpc::new([response.as_str()]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let output = futures::executor::block_on(execute_no_args(
		&rpc,
		"Resources",
		"get_lite_stmt_store_slots_per_period",
		id,
		None,
	))
	.unwrap();

	assert_eq!(output, vec![10, 0, 0, 0]);
	assert_eq!(
		scripted.calls(),
		vec![(
			"state_call".to_string(),
			format!("[\"{EXECUTE_VIEW_FUNCTION}\",\"0x{}00\"]", hex::encode(id)),
		)],
	);
}

#[test]
fn a_dispatch_failure_is_not_mistaken_for_an_output() {
	let response = Value::String(format!(
		"0x{}",
		hex::encode(
			Result::<Vec<u8>, ViewFunctionDispatchError>::Err(
				ViewFunctionDispatchError::NotImplemented,
			)
			.encode()
		)
	));

	let error = ViewFunctionError::decode_response("Resources", "get_value", response)
		.unwrap_err()
		.to_string();

	assert!(error.contains("dispatch failed: NotImplemented"), "{error}");
}

#[test]
fn successful_reads_are_cached() {
	let mut metadata = Metadata::decode(FIXTURE_V16).unwrap();
	let definition = metadata.view_function("Resources", "current_stmt_store_period").unwrap();
	metadata.insert_view_function("Resources", "get_stmt_store_slots_per_period", definition);
	let success = response(Ok(7u32.encode()));
	let scripted = ScriptedRpc::new([success.as_str()]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let first = futures::executor::block_on(read_u32(
		&rpc,
		&metadata,
		"Resources",
		"get_stmt_store_slots_per_period",
		None,
	))
	.unwrap();
	let second = futures::executor::block_on(read_u32(
		&rpc,
		&metadata,
		"Resources",
		"get_stmt_store_slots_per_period",
		None,
	))
	.unwrap();

	assert_eq!((first, second), (7, 7));
	assert_eq!(scripted.calls().len(), 1);
}

#[test]
fn failed_reads_are_not_cached() {
	let mut metadata = Metadata::decode(FIXTURE_V16).unwrap();
	let definition = metadata.view_function("Resources", "current_stmt_store_period").unwrap();
	metadata.insert_view_function("Resources", "get_stmt_store_replacement_cooldown", definition);
	let failure = response(Err(ViewFunctionDispatchError::NotImplemented));
	let success = response(Ok(8u32.encode()));
	let scripted = ScriptedRpc::new([failure.as_str(), success.as_str()]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	assert!(
		futures::executor::block_on(read_u32(
			&rpc,
			&metadata,
			"Resources",
			"get_stmt_store_replacement_cooldown",
			None,
		))
		.is_err()
	);
	let retried = futures::executor::block_on(read_u32(
		&rpc,
		&metadata,
		"Resources",
		"get_stmt_store_replacement_cooldown",
		None,
	))
	.unwrap();

	assert_eq!(retried, 8);
	assert_eq!(scripted.calls().len(), 2);
}

#[test]
fn support_requires_a_no_argument_unsigned_view() {
	let mut metadata = Metadata::decode(FIXTURE_V16).unwrap();
	let definition = metadata.view_function("Resources", "current_stmt_store_period").unwrap();
	metadata.insert_view_function(
		"Resources",
		"get_stmt_store_slots_per_period",
		ViewFunctionDef { inputs: 1, ..definition },
	);

	assert!(!supports_resource_u32(&metadata, "get_stmt_store_slots_per_period",));
	assert!(supports_resource_u32(&metadata, "get_lite_stmt_store_slots_per_period",));
}

#[test]
fn pinned_policy_reads_bypass_best_head_cache() {
	let mut metadata = Metadata::decode(FIXTURE_V16).unwrap();
	let definition = metadata.view_function("Resources", "current_stmt_store_period").unwrap();
	metadata.insert_view_function("Resources", "get_stmt_store_slots_per_period", definition);
	let best = response(Ok(7u32.encode()));
	let pinned = response(Ok(9u32.encode()));
	let scripted = ScriptedRpc::new([best.as_str(), pinned.as_str()]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));
	assert_eq!(
		futures::executor::block_on(read_resource_u32(
			&rpc,
			&metadata,
			"get_stmt_store_slots_per_period",
		))
		.unwrap(),
		7
	);
	assert_eq!(
		futures::executor::block_on(read_resource_u32_at(
			&rpc,
			&metadata,
			"get_stmt_store_slots_per_period",
			"0x42",
		))
		.unwrap(),
		9
	);
	assert!(scripted.calls()[1].1.ends_with(",\"0x42\"]"));
}
