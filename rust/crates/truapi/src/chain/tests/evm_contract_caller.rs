use std::{
	collections::VecDeque,
	sync::atomic::{AtomicUsize, Ordering},
};

use parity_scale_codec::{Compact, Decode};
use subxt::{
	Metadata, SubstrateConfig,
	dynamic::Value as ScaleValue,
	events::Phase,
	ext::{
		frame_decode::extrinsics::decode_extrinsic,
		scale_encode::{EncodeAsFields, Field},
	},
	tx::Signer,
	utils::{AccountId32, Era, MultiSignature},
};

use super::*;
use crate::{
	chain::{
		ContractResult, DEFAULT_MORTALITY_PERIOD, EvmContractCaller, EvmContractError,
		ExecReturnValue, ExtrinsicService, MortalExtrinsic, Mortality, RawDispatchError,
		RuntimeApiCaller, StorageDeposit, Weight,
	},
	host_internal::extrinsic::{Sr25519Signer, tests::split_v4},
};

const ASSET_HUB_METADATA: &[u8] =
	include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");
const CONTRACT: [u8; 20] = [0x44; 20];
const INPUT: &[u8] = &[1, 2, 3, 4];

struct RecordingSigner {
	inner: Sr25519Signer,
	signatures: AtomicUsize,
}

impl RecordingSigner {
	fn new() -> Self {
		let keypair = schnorrkel::MiniSecretKey::from_bytes(&[3; 32])
			.unwrap()
			.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
		Self { inner: Sr25519Signer::from_keypair(&keypair), signatures: AtomicUsize::new(0) }
	}
}

impl Signer<SubstrateConfig> for RecordingSigner {
	fn account_id(&self) -> AccountId32 {
		self.inner.account_id()
	}

	fn sign(&self, payload: &[u8]) -> MultiSignature {
		self.signatures.fetch_add(1, Ordering::SeqCst);
		self.inner.sign(payload)
	}
}

enum Outcome {
	Succeeded,
	DispatchFailed,
	/// `ExtrinsicFailed` with `DispatchError::Trie`, which the runtime has and
	/// subxt's `DispatchError` lacks.
	UndecodableDispatchError,
	MissingEvent,
	MissingExtrinsic,
	Dropped,
	SubmitError,
}

struct EvmNode {
	node: Node,
	output: Vec<u8>,
	simulations: Vec<Vec<u8>>,
	submissions: Vec<Vec<u8>>,
	outcomes: VecDeque<Outcome>,
}

impl EvmNode {
	fn new() -> Self {
		let mut node = Node::new(4, 12);
		node.metadata = ASSET_HUB_METADATA;
		node.account_next_index = 7;
		Self {
			node,
			output: simulation_result(Ok(ExecReturnValue { flags: 0, data: vec![0x80] })),
			simulations: Vec::new(),
			submissions: Vec::new(),
			outcomes: VecDeque::new(),
		}
	}

	fn respond(&mut self, request: &str) -> Vec<String> {
		let parsed: Value = serde_json::from_str(request).unwrap();
		let params = &parsed["params"];
		let response = |result: Value| {
			json!({"jsonrpc": "2.0", "id": parsed["id"], "result": result}).to_string()
		};
		match parsed["method"].as_str().unwrap() {
			"state_call" if params[0] == "ReviveApi_call" => {
				self.simulations.push(hex_bytes(&params[1]));
				vec![response(json!(format!("0x{}", hex::encode(&self.output))))]
			},
			"transactionWatch_v1_submitAndWatch" => {
				let bytes = hex_bytes(&params[0]);
				self.submissions.push(bytes.clone());
				let outcome = self.outcomes.pop_front().unwrap_or(Outcome::Succeeded);
				if matches!(outcome, Outcome::SubmitError) {
					let error = json!({"code": -32603, "message": "scripted submit error"});
					return vec![
						json!({"jsonrpc": "2.0", "id": parsed["id"], "error": error}).to_string(),
					];
				}
				let subscription = format!("contract-{}", self.submissions.len());
				let notification = |result: Value| {
					json!({
						"jsonrpc": "2.0",
						"method": "transactionWatch_v1_watchEvent",
						"params": {"subscription": subscription, "result": result},
					})
					.to_string()
				};
				let mut frames = vec![response(json!(subscription))];
				let parent = self.node.block(self.node.finalized).hash;
				let block = self.node.block(self.node.finalized + 1).hash;
				let body = if matches!(outcome, Outcome::MissingExtrinsic) {
					Vec::new()
				} else {
					vec![bytes.clone()]
				};
				self.node.bodies.insert(block, body);
				let dispatch = match outcome {
					Outcome::Succeeded | Outcome::Dropped => Some(Ok(())),
					Outcome::DispatchFailed => Some(Err(contract_reverted())),
					Outcome::UndecodableDispatchError => {
						let cause = ScaleValue::unnamed_variant("InvalidStateRoot", []);
						Some(Err(ScaleValue::unnamed_variant("Trie", [cause])))
					},
					Outcome::MissingEvent | Outcome::MissingExtrinsic => None,
					Outcome::SubmitError => unreachable!(),
				};
				self.node.events.insert(block, dispatch_events(dispatch));
				frames.extend([
					follow_event(json!({"event": "newBlock", "blockHash": block, "parentBlockHash": parent, "newRuntime": null})),
					follow_event(json!({"event": "bestBlockChanged", "bestBlockHash": block})),
					notification(json!({"event": "bestChainBlockIncluded", "block": {"hash": block, "index": "0"}})),
				]);
				if matches!(outcome, Outcome::Dropped) {
					frames
						.push(notification(json!({"event": "dropped", "error": "scripted stop"})));
					return frames;
				}
				self.node.finalized += 1;
				frames.extend([
					follow_event(
						json!({"event": "finalized", "finalizedBlockHashes": [block], "prunedBlockHashes": []}),
					),
					notification(
						json!({"event": "finalized", "block": {"hash": block, "index": "0"}}),
					),
				]);
				frames
			},
			_ => self.node.respond(request),
		}
	}
}

fn serve_contracts(
	node: EvmNode,
) -> (Arc<Mutex<EvmNode>>, Arc<ScriptedProvider>, EvmContractCaller) {
	let node = Arc::new(Mutex::new(node));
	let scripted = node.clone();
	let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
		scripted.lock().unwrap().respond(request)
	}));
	let chain = ChainRuntime::new(provider.clone(), test_spawner());
	let extrinsics = ExtrinsicService::over_chain(chain.clone(), Arc::new(V4OnlyHost));
	let caller = EvmContractCaller::new(chain, extrinsics, GENESIS);
	(node, provider, caller)
}

fn hex_bytes(value: &Value) -> Vec<u8> {
	hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap()
}

fn simulation_result(result: Result<ExecReturnValue, RawDispatchError>) -> Vec<u8> {
	ContractResult {
		_weight_consumed: Weight { ref_time: 10, proof_size: 20 },
		weight_required: Weight { ref_time: 300, proof_size: 400 },
		storage_deposit: StorageDeposit::Refund { _amount: 500 },
		max_storage_deposit: StorageDeposit::Charge(600),
		_gas_consumed: 70,
		result,
	}
	.encode()
}

/// `DispatchError::Module` for `Revive::ContractReverted`.
fn contract_reverted() -> ScaleValue {
	let metadata = Metadata::decode_from(ASSET_HUB_METADATA).unwrap();
	let revive = metadata.pallet_by_name("Revive").unwrap();
	let reverted = revive
		.error_variants()
		.unwrap()
		.iter()
		.find(|error| error.name == "ContractReverted")
		.unwrap();
	let module_error = ScaleValue::unnamed_composite([
		ScaleValue::u128(revive.error_index().into()),
		ScaleValue::unnamed_composite(
			[reverted.index, 0, 0, 0].map(|byte| ScaleValue::u128(byte.into())),
		),
	]);
	ScaleValue::unnamed_variant("Module", [module_error])
}

/// `System.Events` with the event reporting `dispatch` of the first
/// extrinsic, or no events when `dispatch` is `None`.
fn dispatch_events(dispatch: Option<Result<(), ScaleValue>>) -> Vec<u8> {
	let Some(dispatch) = dispatch else {
		return Compact(0u32).encode();
	};
	let metadata = Metadata::decode_from(ASSET_HUB_METADATA).unwrap();
	let system = metadata.pallet_by_name("System").unwrap();
	let info = ScaleValue::unnamed_composite([
		ScaleValue::unnamed_composite([ScaleValue::u128(0), ScaleValue::u128(0)]),
		ScaleValue::unnamed_variant("Normal", []),
		ScaleValue::unnamed_variant("Yes", []),
	]);
	let (event_name, values) = match dispatch {
		Ok(()) => ("ExtrinsicSuccess", vec![info]),
		Err(error) => ("ExtrinsicFailed", vec![error, info]),
	};
	let event = system
		.event_variants()
		.unwrap()
		.iter()
		.find(|event| event.name == event_name)
		.unwrap();
	let mut bytes =
		(Compact(1u32), Phase::ApplyExtrinsic(0), system.event_index(), event.index).encode();
	let mut fields =
		event.fields.iter().map(|field| Field::new(field.ty.id, field.name.as_deref()));
	ScaleValue::unnamed_composite(values)
		.encode_as_fields_to(&mut fields, metadata.types(), &mut bytes)
		.unwrap();
	Vec::<[u8; 32]>::new().encode_to(&mut bytes);
	bytes
}

#[derive(Debug, PartialEq)]
struct Submission {
	account: [u8; 32],
	nonce: u64,
	era: Era,
	pallet: String,
	call: String,
	arguments: Vec<u8>,
}

fn decoded_submission(bytes: &[u8]) -> Submission {
	let metadata = Metadata::decode_from(ASSET_HUB_METADATA).unwrap();
	let decoded = decode_extrinsic(&mut &bytes[..], &metadata, metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let extension = |name| {
		let field = extensions.iter().find(|field| field.name() == name).unwrap();
		&bytes[field.range()]
	};
	Submission {
		account: split_v4(bytes).0,
		nonce: Compact::<u64>::decode(&mut extension("CheckNonce")).unwrap().0,
		era: Era::decode(&mut extension("CheckMortality")).unwrap(),
		pallet: decoded.pallet_name().to_string(),
		call: decoded.call_name().to_string(),
		arguments: bytes[decoded.call_data_args_range()].to_vec(),
	}
}

fn expected_call(signer: &RecordingSigner, nonce: u64, birth: u64) -> Submission {
	Submission {
		account: signer.account_id().0,
		nonce,
		era: Era::mortal(64, birth),
		pallet: "Revive".to_string(),
		call: "call".to_string(),
		arguments: (
			CONTRACT,
			Compact(0u128),
			Compact(300u64),
			Compact(400u64),
			Compact(600u128),
			INPUT.to_vec(),
		)
			.encode(),
	}
}

/// What the node and the signer were asked to do.
#[derive(Debug, PartialEq)]
struct Activity {
	simulations: Vec<Vec<u8>>,
	nonce_reads: usize,
	submissions: Vec<Submission>,
	signatures: usize,
}

fn activity(
	node: &Mutex<EvmNode>,
	provider: &ScriptedProvider,
	signer: &RecordingSigner,
) -> Activity {
	let node = node.lock().unwrap();
	Activity {
		simulations: node.simulations.clone(),
		nonce_reads: method_count(provider, "system_accountNextIndex"),
		submissions: node.submissions.iter().map(|bytes| decoded_submission(bytes)).collect(),
		signatures: signer.signatures.load(Ordering::SeqCst),
	}
}

#[test]
fn preparation_simulates_the_signer_without_signing_or_submitting() {
	let signer = RecordingSigner::new();
	let (node, provider, caller) = serve_contracts(EvmNode::new());

	let _prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

	let expected = (
		signer.account_id().0,
		CONTRACT,
		0u128,
		Option::<(Compact<u64>, Compact<u64>)>::None,
		Option::<u128>::None,
		INPUT.to_vec(),
	)
		.encode();
	assert_eq!(
		activity(&node, &provider, &signer),
		Activity {
			simulations: vec![expected],
			nonce_reads: 0,
			submissions: Vec::new(),
			signatures: 0,
		},
	);
}

#[test]
fn execution_uses_the_prepared_call_and_limits_with_a_fresh_nonce_and_era() {
	let signer = RecordingSigner::new();
	let (node, provider, caller) = serve_contracts(EvmNode::new());
	let mut input = INPUT.to_vec();
	let prepared = block_on(caller.dry_run(&signer, &CONTRACT, &input)).unwrap();
	input.fill(0xff);
	let simulations = {
		let mut node = node.lock().unwrap();
		node.output = simulation_result(Ok(ExecReturnValue { flags: 1, data: vec![0x80] }));
		node.node.account_next_index = 19;
		node.node.finalized = 6;
		node.simulations.clone()
	};

	block_on(prepared.execute_until_finalized()).unwrap();

	assert_eq!(
		activity(&node, &provider, &signer),
		Activity {
			simulations,
			nonce_reads: 1,
			submissions: vec![expected_call(&signer, 19, 6)],
			signatures: 1,
		},
	);
}

/// A revert during execution, running out of gas and an exhausted deposit
/// limit all fail the dispatch; only the module error tells them apart. The
/// event alone proves the failure, so an error that does not decode still
/// fails the write, with its bytes.
#[test]
fn a_failed_dispatch_carries_its_dispatch_error() {
	let signer = RecordingSigner::new();
	for (outcome, expected) in [
		(Outcome::DispatchFailed, "Pallet error: Revive::ContractReverted"),
		(Outcome::UndecodableDispatchError, "undecodable dispatch error 0x0e0000000000: "),
	] {
		let mut node = EvmNode::new();
		node.outcomes.push_back(outcome);
		let (_, _, caller) = serve_contracts(node);
		let prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

		let result = block_on(prepared.execute_until_finalized());

		let Err(EvmContractError::DispatchFailed { detail }) = result else {
			panic!("unexpected failed dispatch: {result:?}");
		};
		assert!(detail.starts_with(expected), "{detail}");
	}
}

/// A dropped extrinsic, or one whose submission returned an error, may have
/// reached the node and can still be included until its era ends, so the
/// caller gets it back to follow instead of signing the write again. A best
/// block including the dropped one does not finalize the write.
#[test]
fn an_unfinalized_write_returns_the_submitted_extrinsic_and_its_era() {
	let signer = RecordingSigner::new();
	let submit_error = "submit_and_watch: Cannot submit extrinsic: Backend error: RPC error: RPC \
		error: User error: scripted submit error (-32603)";
	for (outcome, expected) in [
		(Outcome::Dropped, WatchEvent::Dropped("scripted stop".to_string())),
		(Outcome::SubmitError, WatchEvent::Error(submit_error.to_string())),
	] {
		let mut node = EvmNode::new();
		node.outcomes.push_back(outcome);
		let (node, _, caller) = serve_contracts(node);
		let prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

		let result = block_on(prepared.execute_until_finalized());

		let Err(EvmContractError::NotFinalized { extrinsic, event }) = result else {
			panic!("unexpected unfinalized write: {result:?}");
		};
		let node = node.lock().unwrap();
		let submitted = MortalExtrinsic {
			extrinsic: EncodedExtrinsic::new(node.submissions[0].clone()),
			mortality: Mortality::new(node.node.block(4), DEFAULT_MORTALITY_PERIOD).unwrap(),
		};
		assert_eq!((*extrinsic, event), (submitted, expected));
	}
}

/// The watch reported the extrinsic finalized, so an outcome that cannot be
/// read in that block returns the block and the extrinsic to follow instead
/// of reading like a write that can be signed again.
#[test]
fn an_unreadable_outcome_names_the_finalized_block_the_watch_reported() {
	let signer = RecordingSigner::new();
	for (outcome, cause) in [
		(Outcome::MissingEvent, "no dispatch event"),
		(Outcome::MissingExtrinsic, "the finalized block lacks the extrinsic"),
	] {
		let mut node = EvmNode::new();
		node.outcomes.push_back(outcome);
		let (node, _, caller) = serve_contracts(node);
		let prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

		let result = block_on(prepared.execute_until_finalized());

		let (extrinsic, block) = match result {
			Err(EvmContractError::OutcomeUnreadable { extrinsic, block, reason })
				if reason.reason().contains(cause) =>
			{
				(extrinsic, block)
			},
			other => panic!("unexpected unreadable outcome: {other:?}"),
		};
		let node = node.lock().unwrap();
		let submitted = MortalExtrinsic {
			extrinsic: EncodedExtrinsic::new(node.submissions[0].clone()),
			mortality: Mortality::new(node.node.block(4), DEFAULT_MORTALITY_PERIOD).unwrap(),
		};
		assert_eq!((*extrinsic, block), (submitted, node.node.block(node.node.finalized).hash));
	}
}

/// Answers every runtime API call with the same result.
struct ScriptedRuntimeApi(Result<Vec<u8>, String>);

#[async_trait::async_trait]
impl RuntimeApiCaller for ScriptedRuntimeApi {
	async fn call(&mut self, _function: &str, _input: &[u8]) -> Result<Vec<u8>, String> {
		self.0.clone()
	}
}

/// dotNS takes a revert as the contract's answer, so a failed runtime API
/// call, a call the runtime rejects and an output that does not decode must
/// each read as something else.
#[test]
fn only_a_contract_revert_reads_as_reverted() {
	for (answer, expected) in [
		(
			Ok(simulation_result(Ok(ExecReturnValue { flags: 1, data: Vec::new() }))),
			"contract reverted: (empty)",
		),
		(Err("runtime unavailable".to_string()), "ReviveApi_call: runtime unavailable"),
		(
			Ok(simulation_result(Err(RawDispatchError(vec![0])))),
			"ReviveApi_call dispatch failed: 0x00",
		),
		(Ok(vec![0xff]), "ReviveApi_call output is not a ContractResult"),
	] {
		let result =
			block_on(EvmContractCaller::read(&mut ScriptedRuntimeApi(answer), &CONTRACT, INPUT));

		assert_eq!(result.map_err(|error| error.to_string()), Err(expected.to_string()));
	}
}
