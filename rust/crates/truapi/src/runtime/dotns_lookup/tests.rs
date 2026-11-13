use super::*;
use futures::{FutureExt, pin_mut};
use parity_scale_codec::Encode;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::{
	Spawner,
	chain::Weight,
	test_support::{ScriptedProvider, legacy_asset_hub_frames, view_output},
};

fn hex_of(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

/// Every read of a lookup lands on the one best block it found, its views
/// start together, and nothing it sends is a `chainHead_v1` method, so no
/// number of lookups can take a follow from the connection.
#[test]
fn a_lookup_reads_one_best_block_without_a_follow() {
	let sent = Arc::new(Mutex::new(Vec::<Value>::new()));
	let recorded = sent.clone();
	let best = hex_of(&[0x22; 32]);
	let answered_at = best.clone();
	let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
		let request: Value = serde_json::from_str(request).unwrap();
		let reply = |id: &Value, result: Value| {
			json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
		};
		let mut sent = recorded.lock().unwrap();
		sent.push(request.clone());
		match request["method"].as_str().unwrap() {
			"chain_getBlockHash" => vec![reply(&request["id"], json!(answered_at))],
			"state_getStorage" => vec![reply(&request["id"], json!(hex_of(&[0x07])))],
			"state_call" => {
				let calls: Vec<&Value> =
					sent.iter().filter(|request| request["method"] == "state_call").collect();
				if calls.len() < 3 {
					return vec![];
				}
				calls
					.iter()
					.enumerate()
					.rev()
					.map(|(index, call)| {
						let data = vec![u8::try_from(index).unwrap()];
						reply(&call["id"], json!(hex_of(&view_output(data, false))))
					})
					.collect()
			},
			method => panic!("unexpected RPC method: {method}"),
		}
	}));
	let chain = ChainRuntime::new(provider, Spawner::for_test());
	futures::executor::block_on(async {
		let mut lookup = DotnsLookup::at_best_block(&chain, [0x11; 32]).await.unwrap();
		let value = lookup.storage(vec![0xab]).await.unwrap();
		let batch = lookup.views(&[0x44; 20], (0..3).map(|index| vec![index]).collect());
		pin_mut!(batch);
		let ready = batch.as_mut().now_or_never();
		let started = sent.lock().unwrap().iter().filter(|r| r["method"] == "state_call").count();
		assert_eq!(started, 3, "all views start before any reply");
		let outcomes = match ready {
			Some(outcomes) => outcomes,
			None => batch.await,
		};

		let calls: Vec<Value> = sent
			.lock()
			.unwrap()
			.iter()
			.filter(|request| request["method"] == "state_call")
			.map(|request| request["params"].clone())
			.collect();
		let expected_calls: Vec<Value> = (0u8..3)
			.map(|index| {
				let input =
					([0xeeu8; 32], [0x44u8; 20], 0u128, None::<Weight>, None::<u128>, vec![index])
						.encode();
				json!(["ReviveApi_call", hex_of(&input), best])
			})
			.collect();
		let storage_params: Vec<Value> = sent
			.lock()
			.unwrap()
			.iter()
			.filter(|request| request["method"] == "state_getStorage")
			.map(|request| request["params"].clone())
			.collect();
		assert_eq!(
			(
				value,
				storage_params,
				outcomes.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
				calls
			),
			(
				Some(vec![0x07]),
				vec![json!([hex_of(&[0xab]), best])],
				vec![vec![0], vec![1], vec![2]],
				expected_calls
			)
		);
		assert!(
			sent.lock()
				.unwrap()
				.iter()
				.all(|request| !request["method"].as_str().unwrap().starts_with("chainHead_")),
			"a dotNS lookup sent a chainHead method"
		);
	});
}

/// A burst of lookups for different names, more than a node's follow budget,
/// all resolve over the one connection and send no `chainHead_v1` method.
#[test]
fn a_burst_of_different_names_all_resolve() {
	let provider = Arc::new(ScriptedProvider::with_frames(|request| {
		legacy_asset_hub_frames(
			request,
			|key| Some(key.to_vec()),
			|_, input| view_output(input.to_vec(), false),
		)
	}));
	let chain = ChainRuntime::new(provider, Spawner::for_test());
	let outcomes = futures::executor::block_on(futures::future::join_all((0u8..8).map(|name| {
		let chain = &chain;
		async move {
			let mut lookup = DotnsLookup::at_best_block(chain, [0x11; 32]).await.unwrap();
			let record = lookup.storage(vec![name]).await.unwrap();
			let view = lookup.view(&[0x44; 20], vec![name]).await.unwrap();
			(record, view)
		}
	})));
	assert_eq!(outcomes, (0u8..8).map(|name| (Some(vec![name]), vec![name])).collect::<Vec<_>>());
}

/// A node that never names its best block fails the lookup after one step's
/// budget instead of holding its caller.
#[test]
fn a_lookup_with_no_best_block_fails_in_one_step() {
	let provider = Arc::new(ScriptedProvider::with_frames(|request| {
		let request: Value = serde_json::from_str(request).unwrap();
		match request["method"].as_str().unwrap() {
			"chain_getBlockHash" => vec![],
			method => panic!("unexpected RPC method: {method}"),
		}
	}));
	let chain = ChainRuntime::new(provider, Spawner::for_test());
	let error = futures::executor::block_on(DotnsLookup::at_best_block(&chain, [0x11; 32]))
		.err()
		.expect("no best block, no lookup");
	assert_eq!(
		error.to_string(),
		"finding the best Asset Hub block timed out",
		"{error:?}"
	);
}
