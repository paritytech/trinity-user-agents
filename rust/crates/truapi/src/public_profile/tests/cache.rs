use parity_scale_codec::Encode;
use serde_json::{Value, json};
use subxt::utils::WrapperKeepOpaque;

use super::*;
use crate::chain::{ContractResult, ExecReturnValue, StorageDeposit, Weight};

fn profile_responses(record: &ProfileRecord) -> Vec<String> {
	let block = format!("0x{}", hex::encode([0x22; 32]));
	let response = |id, result: Value| {
		json!({"jsonrpc": "2.0", "id": format!("truapi:{id}"), "result": result}).to_string()
	};
	let event = |result: Value| {
		json!({
			"jsonrpc": "2.0", "method": "chainHead_v1_followEvent",
			"params": {"subscription": "profile", "result": result},
		})
		.to_string()
	};
	let mut frames = vec![
		response(1, json!("profile")),
		event(
			json!({"event": "initialized", "finalizedBlockHashes": [block], "finalizedBlockRuntime": null}),
		),
		event(json!({"event": "bestBlockChanged", "bestBlockHash": block})),
		response(2, json!({"result": "started", "operationId": "storage"})),
		event(json!({
			"event": "operationStorageItems", "operationId": "storage",
			"items": [{"key": format!("0x{}", hex::encode(dispatcher_address_key())), "value": format!("0x{}", hex::encode([0x11; 20]))}],
		})),
		event(json!({"event": "operationStorageDone", "operationId": "storage"})),
	];
	let results = [
		DotnsPopController::protocolRegistryCall::abi_encode_returns(&Address::repeat_byte(0x22)),
		ProtocolRegistry::tldCall::abi_encode_returns(&".paseo".to_string()),
		ProtocolRegistry::getCall::abi_encode_returns(&Address::repeat_byte(0x33)),
		ProtocolRegistry::getCall::abi_encode_returns(&Address::repeat_byte(0x44)),
		ContentResolver::textCall::abi_encode_returns(&STANDARD.encode(record.encode())),
	];
	for (index, data) in results.into_iter().enumerate() {
		let operation = format!("call-{index}");
		let output = ContractResult {
			_weight_consumed: Weight { ref_time: 0, proof_size: 0 },
			weight_required: Weight { ref_time: 0, proof_size: 0 },
			storage_deposit: StorageDeposit::Charge(0),
			max_storage_deposit: StorageDeposit::Charge(0),
			_gas_consumed: 0,
			result: Ok(ExecReturnValue { flags: 0, data }),
		}
		.encode();
		frames.push(response(index + 3, json!({"result": "started", "operationId": operation})));
		frames.push(event(json!({
			"event": "operationCallDone", "operationId": operation,
			"output": format!("0x{}", hex::encode(output)),
		})));
	}
	frames
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
			rpc_responses: profile_responses(&record),
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
