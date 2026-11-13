//! An Asset Hub node answering the legacy JSON-RPC reads a dotNS lookup makes:
//! its best block, storage values and `ReviveApi_call` views, each checked to
//! be read at that block.

use parity_scale_codec::{Decode, Encode};
use serde_json::{Value, json};

use crate::chain::{ContractResult, ExecReturnValue, StorageDeposit, Weight};

/// The best block every scripted read must name.
const BEST_BLOCK: [u8; 32] = [0x22; 32];

/// The `ReviveApi_call` output of a view that returns `data`, or reverts with
/// it when `reverted`.
pub fn view_output(data: Vec<u8>, reverted: bool) -> Vec<u8> {
	ContractResult {
		_weight_consumed: Weight::default(),
		weight_required: Weight::default(),
		storage_deposit: StorageDeposit::Charge(0),
		max_storage_deposit: StorageDeposit::Charge(0),
		_gas_consumed: 0,
		result: Ok(ExecReturnValue { flags: u32::from(reverted), data }),
	}
	.encode()
}

/// Answers one legacy request of a dotNS lookup. `storage` answers a storage
/// key, and `view` answers a contract view with its whole `ReviveApi_call`
/// output (see [`view_output`]). Panics on any other method, so a lookup that
/// reached for a follow fails the test.
pub fn legacy_asset_hub_frames(
	request: &str,
	storage: impl FnOnce(&[u8]) -> Option<Vec<u8>>,
	view: impl FnOnce(&[u8; 20], &[u8]) -> Vec<u8>,
) -> Vec<String> {
	let request: Value = serde_json::from_str(request).unwrap();
	let id = request.get("id").cloned().unwrap_or(Value::Null);
	let reply = |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
	let head = hex_of(&BEST_BLOCK);
	let params = &request["params"];
	match request["method"].as_str().unwrap() {
		"chain_getBlockHash" => vec![reply(json!(head))],
		"state_getStorage" => {
			assert_eq!(params[1], json!(head), "a dotNS storage read left the best block");
			let value = storage(&bytes_of(&params[0]));
			vec![reply(value.map_or(Value::Null, |value| json!(hex_of(&value))))]
		},
		"state_call" => {
			assert_eq!(params[0], "ReviveApi_call");
			assert_eq!(params[2], json!(head), "a dotNS view left the best block");
			let args = bytes_of(&params[1]);
			// origin[32] ‖ dest[20] ‖ value u128 ‖ None ‖ None ‖ Vec(input).
			assert_eq!(&args[..32], &[0xee; 32], "views run as the always-mapped origin");
			assert_eq!(&args[52..70], &[0u8; 18], "zero value, no gas or deposit limit");
			let dest: [u8; 20] = args[32..52].try_into().unwrap();
			let input = Vec::<u8>::decode(&mut &args[70..]).unwrap();
			vec![reply(json!(hex_of(&view(&dest, &input))))]
		},
		method => panic!("a dotNS lookup sent {method}"),
	}
}

fn hex_of(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

fn bytes_of(value: &Value) -> Vec<u8> {
	hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap()
}
