use super::*;
use crate::host_internal::extrinsic::tests::bulletin_metadata;
use subxt::metadata::ArcMetadata;

#[test]
fn dry_run_classifies_allowance_rejections_for_retry() {
	for validity in [
		ValidationResult::Invalid(TransactionInvalid::Payment),
		ValidationResult::Invalid(TransactionInvalid::Custom(7)),
		ValidationResult::Invalid(TransactionInvalid::BadSigner),
	] {
		assert_eq!(
			BulletinRpc::classify_dry_run_validity(validity).unwrap(),
			DryRunStatus::AllowanceRejected
		);
	}
}

#[test]
fn classifies_post_broadcast_errors() {
	let invalid =
		BulletinSubmitError::from(TransactionStatusError::Invalid("bad nonce".to_string()));
	assert!(invalid.is_node_rejection());
	assert!(invalid.is_invalid_verdict());
	assert!(!invalid.is_broadcast_outcome_unknown(SubmissionPhase::Watch));

	let dropped = BulletinSubmitError::Subxt(Box::new(
		TransactionStatusError::Dropped("pool full".to_string()).into(),
	));
	assert!(dropped.is_node_rejection());
	assert!(!dropped.is_invalid_verdict());
	assert!(!dropped.is_broadcast_outcome_unknown(SubmissionPhase::Watch));

	let interrupted = BulletinSubmitError::Subxt(Box::new(
		TransactionProgressError::UnexpectedEndOfTransactionStatusStream.into(),
	));
	assert!(interrupted.is_broadcast_outcome_unknown(SubmissionPhase::Watch));
	assert!(!interrupted.is_node_rejection());

	let node_error = BulletinSubmitError::Subxt(Box::new(
		TransactionStatusError::Error("node error".to_string()).into(),
	));
	assert!(node_error.is_broadcast_outcome_unknown(SubmissionPhase::Watch));
	assert!(!node_error.is_node_rejection());

	let inconsistent_inclusion = BulletinSubmitError::Subxt(Box::new(
		TransactionEventsError::CannotFindTransactionInBlock {
			block_hash: [1_u8; 32].into(),
			transaction_hash: [2_u8; 32].into(),
		}
		.into(),
	));
	assert!(inconsistent_inclusion.is_broadcast_outcome_unknown(SubmissionPhase::Events));
	assert!(!inconsistent_inclusion.is_broadcast_outcome_unknown(SubmissionPhase::DryRun));

	assert!(
		!BulletinSubmitError::Timeout { phase: SubmissionPhase::Watch }
			.is_broadcast_outcome_unknown(SubmissionPhase::Watch)
	);
}

/// Decode a `DispatchError::Module` for the named error variant out of
/// the bulletin fixture metadata.
fn module_error(error_name: &str) -> DispatchError {
	let metadata = ArcMetadata::from(bulletin_metadata());
	let pallet = metadata.pallet_by_name(STORE_PALLET_NAME).unwrap();
	let variant_index = (0..=u8::MAX)
		.find(|index| {
			pallet
				.error_variant_by_index(*index)
				.is_some_and(|variant| variant.name == error_name)
		})
		.unwrap_or_else(|| panic!("fixture metadata lacks the {error_name} error"));
	// `DispatchError::Module` is variant 3: (pallet index, 4 error bytes).
	let bytes = [3, pallet.error_index(), variant_index, 0, 0, 0];
	DispatchError::decode_from(&bytes, metadata).unwrap()
}

/// Any error variant that is not an allowance rejection.
fn non_allowance_error_name() -> String {
	let metadata = ArcMetadata::from(bulletin_metadata());
	let pallet = metadata.pallet_by_name(STORE_PALLET_NAME).unwrap();
	(0..=u8::MAX)
		.find_map(|index| {
			let name = &pallet.error_variant_by_index(index)?.name;
			(!ALLOWANCE_REJECTED_MODULE_ERRORS.contains(&name.as_str())).then(|| name.clone())
		})
		.expect("fixture metadata has a non-allowance error variant")
}

#[test]
fn dispatch_errors_classify_allowance_rejections() {
	assert!(matches!(
		classify_dispatch_error(module_error("AuthorizationNotFound")),
		BulletinSubmitError::AllowanceRejected { phase: AllowanceRejectionPhase::Dispatch }
	));

	let other = non_allowance_error_name();
	let error = classify_dispatch_error(module_error(&other));
	let BulletinSubmitError::Subxt(error) = error else {
		panic!("non-allowance dispatch error must remain a Subxt error");
	};
	assert!(matches!(
		error.as_ref(),
		subxt::Error::TransactionEventsError(TransactionEventsError::ExtrinsicFailed(_))
	));
}

#[test]
fn error_reason_strings_are_stable() {
	assert_eq!(
		BulletinSubmitError::AllowanceRejected { phase: AllowanceRejectionPhase::DryRun }
			.to_string(),
		"allowance rejected: dry-run"
	);
	assert_eq!(
		BulletinSubmitError::InvalidTransaction(TransactionInvalid::Future).to_string(),
		"invalid: Future"
	);
	assert_eq!(
		BulletinSubmitError::Timeout { phase: SubmissionPhase::Watch }.to_string(),
		"timeout: watch, inclusion unverified"
	);
	assert_eq!(
		BulletinSubmitError::Timeout { phase: SubmissionPhase::Connect }.to_string(),
		"timeout: connect"
	);
	assert_eq!(
		BulletinSubmitError::Subxt(Box::new(
			TransactionStatusError::Invalid("bad nonce".to_string()).into(),
		))
		.to_string(),
		"subxt: The transaction is not valid: bad nonce"
	);
	assert_eq!(
		BulletinSubmitError::RebroadcastRejected { reason: "stale".to_string() }.to_string(),
		"inclusion unverified: re-broadcast rejected: stale"
	);
}

#[cfg(not(target_arch = "wasm32"))]
mod orchestration {
	use super::*;
	use crate::{
		chain_runtime::{RuntimeChainProvider, RuntimeFailure},
		host_internal::extrinsic::tests::{
			OfflineChainState, bulletin_chain_state, bulletin_runtime_call, split_v4, system_events,
		},
		host_logic::product_account::SR25519_SIGNING_CONTEXT,
		platform::JsonRpcConnection,
		subscription::thread_per_subscription_spawner,
	};
	use futures::{StreamExt, channel::mpsc, stream::BoxStream};
	use parity_scale_codec::{Compact, Decode, Encode};
	use schnorrkel::{PublicKey, Signature};
	use serde_json::{Value as JsonValue, json};
	use std::{
		collections::VecDeque,
		sync::{Arc, Mutex},
	};
	use subxt::{
		config::DefaultExtrinsicParamsBuilder,
		utils::{Era, H256},
	};

	const FOLLOW_ID: &str = "bulletin-follow";
	const BLOCK_HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
	const INCLUDED_HASH: &str =
		"0x2222222222222222222222222222222222222222222222222222222222222222";

	#[derive(Clone, Copy)]
	enum TransactionOutcome {
		Included,
		IncludedWithMissingBody,
		Invalid,
		InvalidWithMissingBody,
		Dropped,
		FollowStop,
		FollowStopWithMissingBody,
	}

	#[derive(Clone, Copy, PartialEq, Eq)]
	enum ValidationOutcome {
		Valid,
		AllowanceRejected,
	}

	struct ScriptedState {
		transaction_outcomes: VecDeque<TransactionOutcome>,
		validation_outcomes: VecDeque<ValidationOutcome>,
		next_operation: usize,
		next_transaction: usize,
		next_best_block: usize,
		current_best_hash: String,
		advance_best_block_after_rejection: bool,
		stall_headers: bool,
		replayed_finalized_blocks: usize,
		replay_header_delay: Option<Duration>,
		legacy_finalized_block: Option<usize>,
		last_transaction: Option<String>,
		omit_transaction_from_next_body: bool,
	}

	struct BulletinScriptedProvider {
		state: Arc<Mutex<ScriptedState>>,
		sent: Arc<Mutex<Vec<String>>>,
		sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
		receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
		events: String,
	}

	impl BulletinScriptedProvider {
		fn new(outcomes: impl IntoIterator<Item = TransactionOutcome>) -> Self {
			Self::with_options(outcomes, false)
		}

		fn with_options(
			outcomes: impl IntoIterator<Item = TransactionOutcome>,
			stall_headers: bool,
		) -> Self {
			let (sender, receiver) = mpsc::unbounded();
			Self {
				state: Arc::new(Mutex::new(ScriptedState {
					transaction_outcomes: outcomes.into_iter().collect(),
					validation_outcomes: VecDeque::new(),
					next_operation: 0,
					next_transaction: 0,
					next_best_block: 0,
					current_best_hash: BLOCK_HASH.to_string(),
					advance_best_block_after_rejection: false,
					stall_headers,
					replayed_finalized_blocks: 0,
					replay_header_delay: None,
					legacy_finalized_block: None,
					last_transaction: None,
					omit_transaction_from_next_body: false,
				})),
				sent: Arc::new(Mutex::new(Vec::new())),
				sender: Arc::new(Mutex::new(Some(sender))),
				receiver: Mutex::new(Some(receiver)),
				events: format!("0x{}", hex::encode(success_events())),
			}
		}

		fn with_best_blocks(self, count: usize) -> Self {
			{
				let mut state = self.state.lock().unwrap();
				state.next_best_block = count;
				state.current_best_hash = scripted_block_hash(count);
			}
			self
		}

		/// The follow subscription initializes with `count` finalized blocks after
		/// the first, whose headers answer only after `delay`, so the best-block
		/// replay can close on an older finalized block than the latest one.
		fn with_lagging_finalized_replay(self, count: usize, delay: Duration) -> Self {
			{
				let mut state = self.state.lock().unwrap();
				state.replayed_finalized_blocks = count;
				state.replay_header_delay = Some(delay);
				state.current_best_hash = scripted_block_hash(count);
			}
			self
		}

		/// The build reads block `count` as finalized while the best-block
		/// stream still stands on the first block: the `count` blocks it
		/// announces have headers that answer only after `delay`.
		fn with_finalized_ahead_of_best(self, count: usize, delay: Duration) -> Self {
			{
				let mut state = self.state.lock().unwrap();
				state.next_best_block = count;
				state.current_best_hash = scripted_block_hash(count);
				state.replay_header_delay = Some(delay);
				state.legacy_finalized_block = Some(count);
			}
			self
		}

		fn with_failed_events(mut self) -> Self {
			self.events = format!("0x{}", hex::encode(failed_events()));
			self
		}

		fn with_validation_outcomes(
			self,
			outcomes: impl IntoIterator<Item = ValidationOutcome>,
			advance_best_block_after_rejection: bool,
		) -> Self {
			{
				let mut state = self.state.lock().unwrap();
				state.validation_outcomes = outcomes.into_iter().collect();
				state.advance_best_block_after_rejection = advance_best_block_after_rejection;
			}
			self
		}

		fn method_count(&self, method: &str) -> usize {
			self.sent
				.lock()
				.unwrap()
				.iter()
				.filter(|request| {
					serde_json::from_str::<JsonValue>(request)
						.ok()
						.and_then(|value| {
							value.get("method").and_then(JsonValue::as_str).map(ToOwned::to_owned)
						})
						.as_deref() == Some(method)
				})
				.count()
		}

		fn submitted_transactions(&self) -> Vec<String> {
			self.sent
				.lock()
				.unwrap()
				.iter()
				.filter_map(|request| {
					let value: JsonValue = serde_json::from_str(request).ok()?;
					(value.get("method").and_then(JsonValue::as_str) ==
						Some("transactionWatch_v1_submitAndWatch"))
					.then(|| value["params"][0].as_str().map(ToOwned::to_owned))
					.flatten()
				})
				.collect()
		}

		fn runtime_call_blocks(&self, runtime_method: &str) -> Vec<H256> {
			self.sent
				.lock()
				.unwrap()
				.iter()
				.filter_map(|request| {
					let request: JsonValue = serde_json::from_str(request).unwrap();
					(request["method"] == "chainHead_v1_call" &&
						request["params"][2] == runtime_method)
						.then(|| request["params"][1].as_str().unwrap().parse().unwrap())
				})
				.collect()
		}

		fn runtime_call_count(&self, runtime_method: &str) -> usize {
			self.sent
				.lock()
				.unwrap()
				.iter()
				.filter(|request| {
					serde_json::from_str::<JsonValue>(request)
						.ok()
						.and_then(|value| {
							(value.get("method").and_then(JsonValue::as_str) ==
								Some("chainHead_v1_call"))
							.then(|| {
								value
									.get("params")
									.and_then(|params| params.get(2))
									.and_then(JsonValue::as_str)
									.map(ToOwned::to_owned)
							})
							.flatten()
						})
						.as_deref() == Some(runtime_method)
				})
				.count()
		}
	}

	struct BulletinScriptedConnection {
		state: Arc<Mutex<ScriptedState>>,
		sent: Arc<Mutex<Vec<String>>>,
		sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
		receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
		events: String,
	}

	impl JsonRpcConnection for BulletinScriptedConnection {
		fn send(&self, request: String) {
			self.sent.lock().unwrap().push(request.clone());
			let mut state = self.state.lock().unwrap();
			let delay = state.replay_header_delay.filter(|_| {
				let request: JsonValue = serde_json::from_str(&request).unwrap();
				request["method"] == "chainHead_v1_header" && request["params"][1] != BLOCK_HASH
			});
			let frames = scripted_frames(&request, &self.events, &mut state);
			drop(state);
			let Some(sender) = self.sender.lock().unwrap().clone() else { return };
			let deliver = move || {
				// A delayed frame can outlive the test that requested it.
				for frame in frames {
					let _ = sender.unbounded_send(frame);
				}
			};
			match delay {
				Some(delay) => {
					std::thread::spawn(move || {
						std::thread::sleep(delay);
						deliver();
					});
				},
				None => deliver(),
			}
		}

		fn responses(&self) -> BoxStream<'static, String> {
			self.receiver.lock().unwrap().take().expect("responses called once").boxed()
		}

		fn close(&self) {
			self.sender.lock().unwrap().take();
		}
	}

	#[async_trait::async_trait]
	impl RuntimeChainProvider for BulletinScriptedProvider {
		async fn connect(
			&self,
			_genesis_hash: Vec<u8>,
		) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
			Ok(Arc::new(BulletinScriptedConnection {
				state: self.state.clone(),
				sent: self.sent.clone(),
				sender: self.sender.clone(),
				receiver: Mutex::new(self.receiver.lock().unwrap().take()),
				events: self.events.clone(),
			}))
		}
	}

	fn scripted_frames(request: &str, events: &str, state: &mut ScriptedState) -> Vec<String> {
		let request: JsonValue = serde_json::from_str(request).unwrap();
		let id = request.get("id").cloned().unwrap_or(JsonValue::Null);
		let method = request["method"].as_str().unwrap();
		let response =
			|result: JsonValue| json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
		let follow_event = |result: JsonValue| {
			json!({
				"jsonrpc": "2.0",
				"method": "chainHead_v1_followEvent",
				"params": {"subscription": FOLLOW_ID, "result": result}
			})
			.to_string()
		};

		match method {
			"chainHead_v1_follow" => {
				let mut frames = vec![
					response(json!(FOLLOW_ID)),
					follow_event(json!({
						"event": "initialized",
						"finalizedBlockHashes": std::iter::once(BLOCK_HASH.to_string())
							.chain((1..=state.replayed_finalized_blocks).map(scripted_block_hash))
							.collect::<Vec<_>>(),
						"finalizedBlockRuntime": null
					})),
				];
				let mut parent_hash = BLOCK_HASH.to_string();
				for index in 1..=state.next_best_block {
					let block_hash = scripted_block_hash(index);
					frames.push(follow_event(json!({
						"event": "newBlock", "blockHash": block_hash,
						"parentBlockHash": parent_hash, "newRuntime": null
					})));
					parent_hash = block_hash;
				}
				frames.push(follow_event(json!({
					"event": "bestBlockChanged", "bestBlockHash": state.current_best_hash
				})));
				frames
			},
			"chainHead_v1_header" if state.stall_headers => Vec::new(),
			"chainHead_v1_header" => {
				let hash = request["params"][1].as_str().unwrap();
				let number = if hash == BLOCK_HASH || hash == INCLUDED_HASH {
					1
				} else {
					u32::from_str_radix(hash.trim_start_matches("0x"), 16).unwrap() + 1
				};
				vec![response(json!(encoded_header(number)))]
			},
			"chainHead_v1_call" => {
				state.next_operation += 1;
				let operation_id = format!("call-{}", state.next_operation);
				let runtime_method = request["params"][2].as_str().unwrap();
				let validation_outcome =
					if runtime_method == "TaggedTransactionQueue_validate_transaction" {
						state.validation_outcomes.pop_front().unwrap_or(ValidationOutcome::Valid)
					} else {
						ValidationOutcome::Valid
					};
				let output = if runtime_method == "AccountNonceApi_account_nonce" &&
					request["params"][1] != BLOCK_HASH
				{
					7u32.encode()
				} else {
					runtime_call_output(runtime_method, validation_outcome)
				};
				let mut frames = vec![
					response(json!({"result": "started", "operationId": operation_id})),
					follow_event(json!({
						"event": "operationCallDone",
						"operationId": operation_id,
						"output": format!("0x{}", hex::encode(output))
					})),
				];
				if validation_outcome == ValidationOutcome::AllowanceRejected &&
					state.advance_best_block_after_rejection
				{
					state.next_best_block += 1;
					let block_hash = scripted_block_hash(state.next_best_block);
					let parent_block_hash =
						std::mem::replace(&mut state.current_best_hash, block_hash.clone());
					frames.push(follow_event(json!({
						"event": "newBlock",
						"blockHash": block_hash,
						"parentBlockHash": parent_block_hash,
						"newRuntime": null
					})));
					frames.push(follow_event(json!({
						"event": "bestBlockChanged",
						"bestBlockHash": block_hash
					})));
				}
				frames
			},
			"transactionWatch_v1_submitAndWatch" => {
				state.next_transaction += 1;
				state.last_transaction = request["params"][0].as_str().map(ToOwned::to_owned);
				let subscription_id = format!("tx-{}", state.next_transaction);
				let outcome =
					state.transaction_outcomes.pop_front().expect("scripted transaction outcome");
				let status = match outcome {
					TransactionOutcome::Included => {
						json!({
							"event": "bestChainBlockIncluded",
							"block": {"hash": INCLUDED_HASH, "index": "0"}
						})
					},
					TransactionOutcome::IncludedWithMissingBody => {
						state.omit_transaction_from_next_body = true;
						json!({
							"event": "bestChainBlockIncluded",
							"block": {"hash": INCLUDED_HASH, "index": "0"}
						})
					},
					TransactionOutcome::Invalid | TransactionOutcome::InvalidWithMissingBody => {
						if matches!(outcome, TransactionOutcome::InvalidWithMissingBody) {
							state.omit_transaction_from_next_body = true;
						}
						json!({"event": "invalid", "error": "scripted invalid"})
					},
					TransactionOutcome::Dropped => {
						json!({"event": "dropped", "error": "scripted dropped"})
					},
					TransactionOutcome::FollowStop |
					TransactionOutcome::FollowStopWithMissingBody => {
						// The chainHead follow dies mid-watch instead of the
						// watch reporting a transaction status.
						if matches!(outcome, TransactionOutcome::FollowStopWithMissingBody) {
							state.omit_transaction_from_next_body = true;
						}
						return vec![
							response(json!(subscription_id)),
							follow_event(json!({"event": "stop"})),
						];
					},
				};
				vec![
					response(json!(subscription_id)),
					json!({
						"jsonrpc": "2.0",
						"method": "transactionWatch_v1_watchEvent",
						"params": {"subscription": subscription_id, "result": status}
					})
					.to_string(),
				]
			},
			"chainHead_v1_body" => {
				state.next_operation += 1;
				let operation_id = format!("body-{}", state.next_operation);
				let transaction =
					state.last_transaction.clone().expect("submitted transaction available");
				let transactions = if std::mem::take(&mut state.omit_transaction_from_next_body) {
					vec![]
				} else {
					vec![transaction]
				};
				vec![
					response(json!({"result": "started", "operationId": operation_id})),
					follow_event(json!({
						"event": "operationBodyDone",
						"operationId": operation_id,
						"value": transactions
					})),
				]
			},
			"chainHead_v1_storage" => {
				state.next_operation += 1;
				let operation_id = format!("storage-{}", state.next_operation);
				let key = request["params"][2][0]["key"].clone();
				vec![
					response(json!({"result": "started", "operationId": operation_id})),
					follow_event(json!({
						"event": "operationStorageItems",
						"operationId": operation_id,
						"items": [{"key": key, "value": events}]
					})),
					follow_event(json!({
						"event": "operationStorageDone",
						"operationId": operation_id
					})),
				]
			},
			"chain_getFinalizedHead" => vec![response(json!(legacy_finalized_hash(state)))],
			"chain_getHeader" => {
				vec![response(legacy_header(request["params"][0].as_str().unwrap()))]
			},
			"state_call" => {
				let runtime_method = request["params"][0].as_str().unwrap();
				let output = runtime_call_output(runtime_method, ValidationOutcome::Valid);
				vec![response(json!(format!("0x{}", hex::encode(output))))]
			},
			"system_accountNextIndex" => vec![response(json!(POOL_NONCE))],
			"chainHead_v1_unpin" | "chainHead_v1_unfollow" | "transactionWatch_v1_unwatch" => {
				vec![response(JsonValue::Null)]
			},
			other => {
				vec![
					json!({
						"jsonrpc": "2.0",
						"id": id,
						"error": {"code": -32601, "message": format!("unexpected method {other}")}
					})
					.to_string(),
				]
			},
		}
	}

	fn encoded_header(number: u32) -> String {
		let parent: H256 = match number {
			1 => H256::zero(),
			2 => BLOCK_HASH.parse().unwrap(),
			_ => scripted_block_hash((number - 2) as usize).parse().unwrap(),
		};
		let mut bytes = Vec::new();
		parent.encode_to(&mut bytes);
		Compact(number).encode_to(&mut bytes);
		[0u8; 32].encode_to(&mut bytes);
		[0u8; 32].encode_to(&mut bytes);
		Vec::<u8>::new().encode_to(&mut bytes);
		format!("0x{}", hex::encode(bytes))
	}

	/// The nonce `system_accountNextIndex` answers.
	const POOL_NONCE: u64 = 7;

	/// The newest finalized block the follow subscription announces.
	fn legacy_finalized_hash(state: &ScriptedState) -> String {
		if let Some(block) = state.legacy_finalized_block {
			return scripted_block_hash(block);
		}
		match state.replayed_finalized_blocks {
			0 => BLOCK_HASH.to_string(),
			newest => scripted_block_hash(newest),
		}
	}

	/// A header in the legacy JSON form, numbered as `encoded_header` numbers it.
	fn legacy_header(hash: &str) -> JsonValue {
		let number = if hash == BLOCK_HASH || hash == INCLUDED_HASH {
			1
		} else {
			u32::from_str_radix(hash.trim_start_matches("0x"), 16).unwrap() + 1
		};
		let zero = format!("0x{}", hex::encode([0u8; 32]));
		json!({
			"parentHash": zero,
			"number": format!("0x{number:x}"),
			"stateRoot": zero,
			"extrinsicsRoot": zero,
			"digest": {"logs": []}
		})
	}

	fn scripted_block_hash(index: usize) -> String {
		format!("0x{index:064x}")
	}

	fn runtime_call_output(method: &str, validation_outcome: ValidationOutcome) -> Vec<u8> {
		if let Some(output) = bulletin_runtime_call(method) {
			return output;
		}
		match method {
			"AccountNonceApi_account_nonce" => 0u32.encode(),
			"TaggedTransactionQueue_validate_transaction"
				if validation_outcome == ValidationOutcome::Valid =>
			{
				let mut output = vec![0];
				0u64.encode_to(&mut output);
				Vec::<Vec<u8>>::new().encode_to(&mut output);
				Vec::<Vec<u8>>::new().encode_to(&mut output);
				64u64.encode_to(&mut output);
				true.encode_to(&mut output);
				output
			},
			"TaggedTransactionQueue_validate_transaction" => {
				// Result::Err(TransactionValidityError::Invalid(
				// TransactionInvalid::Payment)).
				vec![1, 0, 1]
			},
			other => panic!("unexpected runtime call {other}"),
		}
	}

	fn success_events() -> Vec<u8> {
		system_events(&[(0, "ExtrinsicSuccess")])
	}

	fn failed_events() -> Vec<u8> {
		system_events(&[(0, "ExtrinsicFailed")])
	}

	fn allowance_fixture() -> BulletinAllowanceKey {
		BulletinAllowanceKey::from_secret_bytes(
			hex::decode(
				"0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
                 573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
			)
			.unwrap(),
		)
		.unwrap()
	}

	fn rpc(provider: Arc<BulletinScriptedProvider>) -> BulletinRpc {
		let chain = ChainRuntime::new(provider, thread_per_subscription_spawner());
		let extrinsics = Arc::new(ExtrinsicService::over_chain(
			chain.clone(),
			Arc::new(crate::chain::V4OnlyHost),
		));
		BulletinRpc::new(chain, extrinsics, [0x42; 32])
			.with_allowance_block_wait_timeout(Duration::from_millis(25))
	}

	fn store_signature_verifies_at(transaction: &[u8], value: &[u8], block_hash: H256) -> bool {
		let (account, signature, tail) = split_v4(transaction);
		let mut extra = &tail[..];
		let Era::Mortal { period, phase } = Era::decode(&mut extra).unwrap() else {
			panic!("store transaction must expire")
		};
		let nonce = Compact::<u64>::decode(&mut extra).unwrap().0;
		let state = OfflineChainState {
			genesis_hash: [0x42; 32],
			spec_version: 1,
			..bulletin_chain_state()
		};
		let client = state.client_at(phase).unwrap();
		let payload = subxt::dynamic::tx(
			"TransactionStorage",
			"store",
			vec![subxt::dynamic::Value::from_bytes(value)],
		);
		let params = DefaultExtrinsicParamsBuilder::<SubstrateConfig>::new()
			.nonce(nonce)
			.mortal_from_unchecked(period, phase, block_hash)
			.build();
		let signer_payload = client
			.tx()
			.create_v4_signable_offline(&payload, params)
			.unwrap()
			.signer_payload()
			.unwrap();
		PublicKey::from_bytes(&account)
			.unwrap()
			.verify_simple(
				SR25519_SIGNING_CONTEXT,
				&signer_payload,
				&Signature::from_bytes(&signature).unwrap(),
			)
			.is_ok()
	}

	#[test]
	fn submit_preimage_anchors_mortality_at_finalized_and_validates_at_best() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::Included]).with_best_blocks(5),
		);
		let finalized_hash = BLOCK_HASH.parse().unwrap();
		let best_hash = scripted_block_hash(5).parse().unwrap();
		let value = b"fork mortality reproduction";

		futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		let transactions = provider.submitted_transactions();
		let transaction = hex::decode(transactions[0].trim_start_matches("0x")).unwrap();
		let (_, _, tail) = split_v4(&transaction);
		let mut extra = &tail[..];
		let era = Era::decode(&mut extra).unwrap();
		let nonce = Compact::<u64>::decode(&mut extra).unwrap().0;

		assert_eq!(era, Era::mortal(64, 1));
		assert_eq!(nonce, POOL_NONCE);
		assert!(store_signature_verifies_at(&transaction, value, finalized_hash));
		assert!(!store_signature_verifies_at(&transaction, value, best_hash));
		assert_eq!(provider.method_count("system_accountNextIndex"), 1);
		assert_eq!(
			provider.runtime_call_blocks("TaggedTransactionQueue_validate_transaction"),
			vec![best_hash]
		);
	}

	#[test]
	fn submit_preimage_builds_at_the_finalized_anchor_when_the_replay_lags_it() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::Included])
				.with_lagging_finalized_replay(3, Duration::from_millis(50)),
		);
		let finalized_hash: H256 = scripted_block_hash(3).parse().unwrap();
		let value = b"lagging replay";

		futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(5),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		let transactions = provider.submitted_transactions();
		let transaction = hex::decode(transactions[0].trim_start_matches("0x")).unwrap();
		assert!(store_signature_verifies_at(&transaction, value, finalized_hash));
		assert_eq!(provider.method_count("system_accountNextIndex"), 1);
		assert_eq!(
			provider.runtime_call_blocks("TaggedTransactionQueue_validate_transaction"),
			vec![finalized_hash]
		);
	}

	/// A transaction validated at a block before its birth block names a
	/// birth the node does not know, so the dry run waits for the best
	/// block to reach it.
	#[test]
	fn submit_preimage_dry_runs_no_earlier_than_the_birth_block() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::Included])
				.with_finalized_ahead_of_best(2, Duration::from_millis(50)),
		);
		let birth_hash: H256 = scripted_block_hash(2).parse().unwrap();
		let value = b"finalized ahead of best";

		futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(5),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(
			provider.runtime_call_blocks("TaggedTransactionQueue_validate_transaction"),
			vec![birth_hash]
		);
	}

	#[test]
	fn submit_preimage_drives_subxt_happy_path() {
		let provider = Arc::new(BulletinScriptedProvider::new([TransactionOutcome::Included]));
		let value = b"scripted bulletin happy path";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		assert_eq!(provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"), 1);
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
		assert_eq!(provider.method_count("chainHead_v1_storage"), 1);
	}

	#[test]
	fn submit_preimage_rebuilds_once_after_node_rejection() {
		let provider = Arc::new(BulletinScriptedProvider::new([
			TransactionOutcome::Dropped,
			TransactionOutcome::Included,
		]));
		let value = b"scripted bulletin retry";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 2);
	}

	#[test]
	fn submit_preimage_recovers_inconsistent_inclusion_via_recheck() {
		let provider =
			Arc::new(BulletinScriptedProvider::new([TransactionOutcome::IncludedWithMissingBody]));
		let value = b"scripted bulletin inconsistent inclusion";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		// The reported inclusion block was inconsistent, but the re-check
		// finds the transaction on chain: no re-broadcast happens.
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
		assert_eq!(provider.method_count("chainHead_v1_body"), 2);
	}

	#[test]
	fn submit_preimage_recovers_finalized_inclusion_after_watch_stop() {
		let provider = Arc::new(BulletinScriptedProvider::new([
			TransactionOutcome::FollowStop,
			TransactionOutcome::FollowStop,
		]));
		let value = b"scripted watch stop recovery";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
		assert!(provider.method_count("chainHead_v1_body") >= 1);
	}

	#[test]
	fn submit_preimage_reports_failed_dispatch_found_by_inclusion_recheck() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::FollowStop]).with_failed_events(),
		);
		let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			b"scripted watch stop failed dispatch",
		))
		.unwrap_err();

		let BulletinSubmitError::Subxt(error) = error else {
			panic!("found-but-failed dispatch must surface the dispatch error, got {error}");
		};
		assert!(matches!(
			error.as_ref(),
			subxt::Error::TransactionEventsError(TransactionEventsError::ExtrinsicFailed(_))
		));
		// The transaction is on chain; a failed dispatch must never
		// trigger a re-broadcast.
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
	}

	#[test]
	fn submit_preimage_rebroadcasts_when_inclusion_recheck_finds_nothing() {
		let provider = Arc::new(BulletinScriptedProvider::new([
			TransactionOutcome::FollowStopWithMissingBody,
			TransactionOutcome::Included,
		]));
		let value = b"scripted watch stop rebroadcast";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		let submitted = provider.submitted_transactions();
		assert_eq!(submitted.len(), 2);
		// The retry re-broadcasts the identical signed bytes; a fresh
		// signing could double-store if the first broadcast landed.
		assert_eq!(submitted[0], submitted[1]);
	}

	#[test]
	fn submit_preimage_recovers_inclusion_when_rebroadcast_bounces() {
		let provider = Arc::new(BulletinScriptedProvider::new([
			TransactionOutcome::FollowStopWithMissingBody,
			TransactionOutcome::Invalid,
		]));
		let value = b"scripted bounced rebroadcast recovery";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		// The bounce triggers one more re-check, which finds the first
		// broadcast on chain; the invalid verdict never surfaces.
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 2);
	}

	#[test]
	fn submit_preimage_reports_unverified_when_rebroadcast_bounces_unseen() {
		let provider = Arc::new(BulletinScriptedProvider::new([
			TransactionOutcome::FollowStopWithMissingBody,
			TransactionOutcome::InvalidWithMissingBody,
		]));
		let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			b"scripted bounced rebroadcast unseen",
		))
		.unwrap_err();

		assert!(matches!(error, BulletinSubmitError::RebroadcastRejected { .. }));
		assert!(error.to_string().starts_with("inclusion unverified"));
		// A bounced re-broadcast must never trigger a rebuilt signing:
		// the first broadcast may be on chain out of the re-check's
		// reach, and a fresh nonce would double-store.
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 2);
	}

	#[test]
	fn submit_preimage_does_not_infer_allowance_rejection_from_authoring_invalidity() {
		let provider = Arc::new(BulletinScriptedProvider::new([
			TransactionOutcome::Invalid,
			TransactionOutcome::Invalid,
		]));
		let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			b"scripted stale allowance",
		))
		.unwrap_err();

		let BulletinSubmitError::Subxt(error) = error else {
			panic!("invalid authoring status must remain a Subxt error");
		};
		assert!(matches!(
			error.as_ref(),
			subxt::Error::TransactionStatusError(TransactionStatusError::Invalid(message))
				if message == "scripted invalid"
		));
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 2);
		assert_eq!(provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"), 2);
	}

	#[test]
	fn submit_preimage_classifies_allowance_only_from_the_follow_up_dry_run() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::Invalid]).with_validation_outcomes(
				[ValidationOutcome::Valid, ValidationOutcome::AllowanceRejected],
				false,
			),
		);
		let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			b"scripted stale allowance",
		))
		.unwrap_err();

		assert!(matches!(
			error,
			BulletinSubmitError::AllowanceRejected { phase: AllowanceRejectionPhase::DryRun }
		));
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
		assert_eq!(provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"), 2);
	}

	#[test]
	fn submit_preimage_budget_reports_pre_broadcast_phase() {
		let provider = Arc::new(BulletinScriptedProvider::with_options([], true));
		let error = futures::executor::block_on(rpc(provider).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_millis(25),
			&allowance_fixture(),
			b"timeout",
		))
		.unwrap_err();

		assert!(matches!(&error, BulletinSubmitError::Timeout { phase: SubmissionPhase::Connect }));
		assert_eq!(error.to_string(), "timeout: connect");
	}

	#[test]
	fn dry_run_rebuilds_at_a_new_best_block_then_submits() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::Included]).with_validation_outcomes(
				[ValidationOutcome::AllowanceRejected, ValidationOutcome::Valid],
				true,
			),
		);
		let value = b"scripted allowance propagation";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		assert_eq!(provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"), 2);
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
	}

	#[test]
	fn dry_run_propagation_block_wait_timeout_remains_an_allowance_rejection() {
		let provider = Arc::new(
			BulletinScriptedProvider::new([])
				.with_validation_outcomes([ValidationOutcome::AllowanceRejected], false),
		);
		let error = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(2),
			&allowance_fixture(),
			b"scripted propagation block wait timeout",
		))
		.unwrap_err();

		assert!(matches!(
			error,
			BulletinSubmitError::AllowanceRejected { phase: AllowanceRejectionPhase::DryRun }
		));
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 0);
	}

	#[test]
	fn dry_run_keeps_polling_past_the_old_block_count_while_blocks_are_fast() {
		// Regression: Bulletin's best-block cadence dropped, so a
		// fixed-count propagation budget (3 blocks) collapsed from the
		// intended ~18s to ~3s and gave up before a freshly-claimed
		// allowance became visible to the dry-run. The wait is now bound
		// by wall-clock, so a rapid stream of rejections keeps polling
		// until the allowance propagates.
		// Six is well past the old three-block give-up cap.
		let rejections_past_old_limit = 6;
		let mut validation_outcomes =
			vec![ValidationOutcome::AllowanceRejected; rejections_past_old_limit];
		validation_outcomes.push(ValidationOutcome::Valid);
		let provider = Arc::new(
			BulletinScriptedProvider::new([TransactionOutcome::Included])
				.with_validation_outcomes(validation_outcomes, true),
		);
		let value = b"scripted fast-block allowance propagation";
		let result = futures::executor::block_on(rpc(provider.clone()).submit_preimage(
			&CallContext::default(),
			Instant::now() + Duration::from_secs(30),
			&allowance_fixture(),
			value,
		))
		.unwrap();

		assert_eq!(result, preimage_key(value));
		assert_eq!(
			provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"),
			rejections_past_old_limit + 1
		);
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 1);
	}

	#[test]
	fn dry_run_propagation_stops_once_the_wall_clock_window_elapses() {
		// A zero-length window makes the first rejection exceed the budget
		// immediately, so a never-propagating allowance is treated as real
		// after one dry-run rather than looping until the outer deadline.
		let provider = Arc::new(
			BulletinScriptedProvider::new([])
				.with_validation_outcomes([ValidationOutcome::AllowanceRejected; 4], true),
		);
		let error = futures::executor::block_on(
			rpc(provider.clone())
				.with_allowance_propagation_window(Duration::ZERO)
				.submit_preimage(
					&CallContext::default(),
					Instant::now() + Duration::from_secs(2),
					&allowance_fixture(),
					b"scripted propagation window elapsed",
				),
		)
		.unwrap_err();

		assert!(matches!(
			error,
			BulletinSubmitError::AllowanceRejected { phase: AllowanceRejectionPhase::DryRun }
		));
		assert_eq!(provider.runtime_call_count("TaggedTransactionQueue_validate_transaction"), 1);
		assert_eq!(provider.method_count("transactionWatch_v1_submitAndWatch"), 0);
	}
}
