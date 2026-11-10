use std::sync::Mutex;

use futures::FutureExt;
use parity_scale_codec::{Decode, Encode};
use serde_json::{Value, json};
use subxt::utils::WrapperKeepOpaque;

use super::*;
use crate::chain::{ContractResult, ExecReturnValue, StorageDeposit, Weight};

fn profile_responder(record: &ProfileRecord) -> crate::test_support::RpcResponder {
	let contracts = Mutex::new(ProfileContracts::new(Ok(STANDARD.encode(record.encode()))));
	Arc::new(move |request| {
		let request: Value = serde_json::from_str(request).unwrap();
		let block = format!("0x{}", hex::encode([0x22; 32]));
		let response = |result: Value| {
			json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string()
		};
		let event = |result: Value| {
			json!({
				"jsonrpc": "2.0", "method": "chainHead_v1_followEvent",
				"params": {"subscription": "profile", "result": result},
			})
			.to_string()
		};
		let operation = request["id"].as_str().unwrap();
		match request["method"].as_str().unwrap() {
			"chainHead_v1_follow" => vec![
				response(json!("profile")),
				event(
					json!({"event": "initialized", "finalizedBlockHashes": [block], "finalizedBlockRuntime": null}),
				),
				event(json!({"event": "bestBlockChanged", "bestBlockHash": block})),
			],
			"chainHead_v1_storage" => vec![
				response(json!({"result": "started", "operationId": operation})),
				event(json!({
					"event": "operationStorageItems", "operationId": operation,
					"items": [{"key": request["params"][2][0]["key"], "value": format!("0x{}", hex::encode([0x11; 20]))}],
				})),
				event(json!({"event": "operationStorageDone", "operationId": operation})),
			],
			"chainHead_v1_call" => {
				let args =
					hex::decode(request["params"][3].as_str().unwrap().trim_start_matches("0x"))
						.unwrap();
				let dest = args[32..52].try_into().unwrap();
				let input = Vec::<u8>::decode(&mut &args[70..]).unwrap();
				let data =
					contracts.lock().unwrap().view(&dest, input).now_or_never().unwrap().unwrap();
				let output = ContractResult {
					_weight_consumed: Weight { ref_time: 0, proof_size: 0 },
					weight_required: Weight { ref_time: 0, proof_size: 0 },
					storage_deposit: StorageDeposit::Charge(0),
					max_storage_deposit: StorageDeposit::Charge(0),
					_gas_consumed: 0,
					result: Ok(ExecReturnValue { flags: 0, data }),
				}
				.encode();
				vec![
					response(json!({"result": "started", "operationId": operation})),
					event(
						json!({"event": "operationCallDone", "operationId": operation, "output": format!("0x{}", hex::encode(output))}),
					),
				]
			},
			"chainHead_v1_unpin" | "chainHead_v1_unfollow" => vec![response(Value::Null)],
			method => panic!("unexpected profile RPC: {method}"),
		}
	})
}

#[tokio::test]
async fn refreshing_an_unchanged_avatar_renews_only_the_profile_cache() {
	for (expires_in, force) in [(0, false), (RECORD_TTL_SECS, true)] {
		let record = ProfileRecord {
			entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
				cid: [9; 32].into(),
				format: crate::ImageFormat::Png,
			}))],
		};
		let platform = Arc::new(StubPlatform {
			rpc_responder: Some(profile_responder(&record)),
			..Default::default()
		});
		let services = services_with_platform(platform.clone());
		let (_directory, db) = crate::store::open_temporary_core_db();
		let now = current_unix_secs();
		let stored = record.clone();
		db.write(move |tx| dao::store_record(tx, "alice.01", &stored, now, now + expires_in))
			.await
			.unwrap();
		assert!(services.install_core_db(db.clone()));

		let profile = PublicProfiles::new(services).resolve("alice.01", force).await.unwrap();
		let (cached, expires_at) = db
			.read(move |conn| {
				let cached = dao::fresh_record(conn, "alice.01", now)?;
				let expires_at: u64 = conn.query_row(
					"SELECT expires_at FROM public_profiles WHERE username = 'alice.01'",
					[],
					|row| row.get(0),
				)?;
				Ok((cached, expires_at))
			})
			.await
			.unwrap();

		assert_eq!(
			(profile, cached, platform.chain_connects.lock().unwrap().clone()),
			(PublicProfile { avatar: record.avatar() }, Some(record), vec![[3; 32]])
		);
		assert!(
			(now + RECORD_TTL_SECS..=current_unix_secs() + RECORD_TTL_SECS).contains(&expires_at)
		);
	}
}
