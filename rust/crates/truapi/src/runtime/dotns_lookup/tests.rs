use super::*;
use futures::{FutureExt, pin_mut};
use parity_scale_codec::Encode;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::{
	chain::{ContractResult, ExecReturnValue, StorageDeposit, Weight},
	test_support::{ScriptedProvider, test_spawner},
};

#[test]
fn a_view_batch_starts_together_on_one_block_and_keeps_early_replies() {
	let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
	let recorded_calls = calls.clone();
	let block = format!("0x{}", hex::encode([0x22; 32]));
	let pinned_block = block.clone();
	let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
		let request: Value = serde_json::from_str(request).unwrap();
		let event = |result: Value| {
			json!({
				"jsonrpc": "2.0",
				"method": "chainHead_v1_followEvent",
				"params": {"subscription": "batch-follow", "result": result},
			})
			.to_string()
		};
		match request["method"].as_str().unwrap() {
			"chainHead_v1_follow" => vec![
				json!({"jsonrpc": "2.0", "id": request["id"], "result": "batch-follow"})
					.to_string(),
				event(json!({
					"event": "initialized",
					"finalizedBlockHashes": [block],
					"finalizedBlockRuntime": null,
				})),
				event(json!({"event": "bestBlockChanged", "bestBlockHash": block})),
			],
			"chainHead_v1_call" => {
				let mut calls = recorded_calls.lock().unwrap();
				calls.push(request);
				if calls.len() < 3 {
					return vec![];
				}
				let mut frames = Vec::new();
				for (index, call) in calls.iter().enumerate().rev() {
					let operation_id = format!("view-{index}");
					let output = ContractResult {
						_weight_consumed: Weight { ref_time: 0, proof_size: 0 },
						weight_required: Weight { ref_time: 0, proof_size: 0 },
						storage_deposit: StorageDeposit::Charge(0),
						max_storage_deposit: StorageDeposit::Charge(0),
						_gas_consumed: 0,
						result: Ok(ExecReturnValue { flags: 0, data: vec![index as u8] }),
					}
					.encode();
					frames.push(event(json!({
						"event": "operationCallDone",
						"operationId": operation_id,
						"output": format!("0x{}", hex::encode(output)),
					})));
					frames.push(
						json!({
							"jsonrpc": "2.0",
							"id": call["id"],
							"result": {"result": "started", "operationId": operation_id},
						})
						.to_string(),
					);
				}
				frames
			},
			"chainHead_v1_unfollow" => {
				vec![json!({"jsonrpc": "2.0", "id": request["id"], "result": null}).to_string()]
			},
			method => panic!("unexpected RPC method: {method}"),
		}
	}));
	let chain = ChainRuntime::new(provider, test_spawner());
	futures::executor::block_on(async {
		let mut lookup =
			DotnsLookup::pinned_to_best_block(&chain, [0x11; 32], DotnsLookupKind::Identity)
				.await
				.unwrap();
		let batch = lookup.views(&[0x44; 20], (0..3).map(|index| vec![index]).collect());
		pin_mut!(batch);
		let ready = batch.as_mut().now_or_never();
		assert_eq!(calls.lock().unwrap().len(), 3, "all views start before any reply");
		let outcomes = match ready {
			Some(outcomes) => outcomes,
			None => batch.await,
		};
		let contexts: Vec<_> =
			calls.lock().unwrap().iter().map(|call| call["params"].clone()).collect();

		let expected_contexts: Vec<_> = (0u8..3)
			.map(|index| {
				let input =
					([0xeeu8; 32], [0x44u8; 20], 0u128, None::<Weight>, None::<u128>, vec![index])
						.encode();
				json!([
					"batch-follow",
					pinned_block,
					"ReviveApi_call",
					format!("0x{}", hex::encode(input))
				])
			})
			.collect();

		assert_eq!(
			(outcomes.into_iter().collect::<Result<Vec<_>, _>>().unwrap(), contexts),
			(vec![vec![0], vec![1], vec![2]], expected_contexts)
		);
	});
}

/// An identity lookup names what it reads and nothing about whose account
/// it reads, so the follow id built from it carries no user identifier
/// into the logs that name a connection's follows.
#[test]
fn an_identity_lookup_is_named_without_an_account() {
	assert_eq!(DotnsLookupKind::Identity.to_string(), "identity");
}

/// A manifest lookup still names its product, which identifies what was
/// opened rather than who opened it.
#[test]
fn a_manifest_lookup_is_named_by_its_product() {
	let product_id = ProductId::for_test("shop.paseo");
	assert_eq!(
		DotnsLookupKind::Manifest(&product_id).to_string(),
		format!("manifest:{product_id}"),
	);
}
