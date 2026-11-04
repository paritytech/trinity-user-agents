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
		ContractResult, EvmContractCaller, EvmContractError, ExecReturnValue, ExtrinsicService,
		RawDispatchError, StorageDeposit, Weight,
	},
	host_internal::extrinsic::{Sr25519Signer, tests::split_v4},
};

const ASSET_HUB_METADATA: &[u8] =
	include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale");
const CONTRACT: [u8; 20] = [0x44; 20];
const INPUT: &[u8] = &[1, 2, 3, 4];

struct RecordingSigner {
	inner: Sr25519Signer,
	account: Mutex<AccountId32>,
	signatures: AtomicUsize,
}

impl RecordingSigner {
	fn new() -> Self {
		let keypair = schnorrkel::MiniSecretKey::from_bytes(&[3; 32])
			.unwrap()
			.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
		let inner = Sr25519Signer::from_keypair(&keypair);
		let account = Mutex::new(inner.account_id());
		Self { inner, account, signatures: AtomicUsize::new(0) }
	}
}

impl Signer<SubstrateConfig> for RecordingSigner {
	fn account_id(&self) -> AccountId32 {
		*self.account.lock().unwrap()
	}

	fn sign(&self, payload: &[u8]) -> MultiSignature {
		self.signatures.fetch_add(1, Ordering::SeqCst);
		self.inner.sign(payload)
	}
}

enum Outcome {
	Succeeded,
	DispatchFailed,
	MissingEvent,
	MissingExtrinsic,
	Stopped(&'static str),
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
			output: simulation_output(0),
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
				let outcome = self.outcomes.pop_front().unwrap_or(Outcome::Succeeded);
				if let Outcome::Stopped(event) = outcome {
					frames.push(notification(json!({"event": event, "error": "scripted stop"})));
					return frames;
				}
				let parent = self.node.block(self.node.finalized).hash;
				self.node.finalized += 1;
				self.node.account_next_index += 1;
				let block = self.node.block(self.node.finalized).hash;
				let body = if matches!(outcome, Outcome::MissingExtrinsic) {
					Vec::new()
				} else {
					vec![bytes.clone()]
				};
				self.node.bodies.insert(block, body);
				let event = match outcome {
					Outcome::Succeeded => Some("ExtrinsicSuccess"),
					Outcome::DispatchFailed => Some("ExtrinsicFailed"),
					Outcome::MissingEvent | Outcome::MissingExtrinsic => None,
					Outcome::Stopped(_) => unreachable!(),
				};
				self.node.events.insert(block, dispatch_events(event));
				frames.extend([
					follow_event(json!({"event": "newBlock", "blockHash": block, "parentBlockHash": parent, "newRuntime": null})),
					follow_event(json!({"event": "bestBlockChanged", "bestBlockHash": block})),
					follow_event(json!({"event": "finalized", "finalizedBlockHashes": [block], "prunedBlockHashes": []})),
					notification(json!({"event": "finalized", "block": {"hash": block, "index": "0"}})),
				]);
				frames
			},
			_ => self.node.respond(request),
		}
	}
}

fn serve_contracts(
	node: EvmNode,
	genesis: H256,
) -> (Arc<Mutex<EvmNode>>, Arc<ScriptedProvider>, EvmContractCaller) {
	let node = Arc::new(Mutex::new(node));
	let scripted = node.clone();
	let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
		scripted.lock().unwrap().respond(request)
	}));
	let chain = ChainRuntime::new(provider.clone(), test_spawner());
	let extrinsics = ExtrinsicService::over_chain(chain.clone(), Arc::new(V4OnlyHost));
	let caller = EvmContractCaller::new(chain, extrinsics, genesis);
	(node, provider, caller)
}

fn hex_bytes(value: &Value) -> Vec<u8> {
	hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap()
}

fn simulation_output(flags: u32) -> Vec<u8> {
	simulation_result(Ok(ExecReturnValue { flags, data: vec![0x80] }))
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

fn dispatch_events(event: Option<&str>) -> Vec<u8> {
	let Some(event_name) = event else {
		return Compact(0u32).encode();
	};
	let metadata = Metadata::decode_from(ASSET_HUB_METADATA).unwrap();
	let system = metadata.pallet_by_name("System").unwrap();
	let event = system
		.event_variants()
		.unwrap()
		.iter()
		.find(|event| event.name == event_name)
		.unwrap();
	let info = ScaleValue::unnamed_composite([
		ScaleValue::unnamed_composite([ScaleValue::u128(0), ScaleValue::u128(0)]),
		ScaleValue::unnamed_variant("Normal", []),
		ScaleValue::unnamed_variant("Yes", []),
	]);
	let values = if event_name == "ExtrinsicFailed" {
		vec![ScaleValue::unnamed_variant("Other", []), info]
	} else {
		vec![info]
	};
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

#[test]
fn preparation_simulates_the_signer_without_signing_or_submitting() {
	let signer = RecordingSigner::new();
	let (node, provider, caller) = serve_contracts(EvmNode::new(), GENESIS);

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
		(
			node.lock().unwrap().simulations.clone(),
			method_count(&provider, "system_accountNextIndex"),
			method_count(&provider, "transactionWatch_v1_submitAndWatch"),
			signer.signatures.load(Ordering::SeqCst),
		),
		(vec![expected], 0, 0, 0),
	);
}

#[test]
fn execution_uses_the_prepared_call_and_limits_with_a_fresh_nonce_and_era() {
	let signer = RecordingSigner::new();
	let (node, provider, caller) = serve_contracts(EvmNode::new(), GENESIS);
	let mut input = INPUT.to_vec();
	let prepared = block_on(caller.dry_run(&signer, &CONTRACT, &input)).unwrap();
	input.fill(0xff);
	{
		let mut node = node.lock().unwrap();
		node.output = simulation_output(1);
		node.node.account_next_index = 19;
		node.node.finalized = 6;
	}

	block_on(caller.execute_and_watch(prepared)).unwrap();

	let submissions: Vec<_> = node
		.lock()
		.unwrap()
		.submissions
		.iter()
		.map(|bytes| decoded_submission(bytes))
		.collect();
	assert_eq!(
		(
			submissions,
			method_count(&provider, "ReviveApi_call"),
			signer.signatures.load(Ordering::SeqCst),
		),
		(vec![expected_call(&signer, 19, 6)], 1, 1),
	);
}

#[test]
fn a_prepared_transaction_cannot_be_sent_to_another_chain() {
	let signer = RecordingSigner::new();
	let (_, _, origin) = serve_contracts(EvmNode::new(), GENESIS);
	let wrong_genesis = H256([0xcd; 32]);
	let (_, provider, wrong_chain) = serve_contracts(EvmNode::new(), wrong_genesis);
	let prepared = block_on(origin.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

	let result = block_on(wrong_chain.execute_and_watch(prepared));

	assert!(
		matches!(result, Err(EvmContractError::WrongChain { expected, actual }) if expected == wrong_genesis && actual == GENESIS),
		"{result:?}",
	);
	assert_eq!(
		(provider.sent.lock().unwrap().clone(), signer.signatures.load(Ordering::SeqCst)),
		(Vec::<String>::new(), 0),
	);
}

#[test]
fn changing_the_signer_account_invalidates_the_prepared_transaction() {
	let signer = RecordingSigner::new();
	let (_, provider, caller) = serve_contracts(EvmNode::new(), GENESIS);
	let prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();
	*signer.account.lock().unwrap() = AccountId32([0x55; 32]);

	let result = block_on(caller.execute_and_watch(prepared));

	assert!(matches!(result, Err(EvmContractError::SignerChanged)), "{result:?}");
	assert_eq!(
		(
			method_count(&provider, "system_accountNextIndex"),
			method_count(&provider, "transactionWatch_v1_submitAndWatch"),
			signer.signatures.load(Ordering::SeqCst),
		),
		(0, 0, 0),
	);
}

#[test]
fn finalized_inclusion_requires_an_explicit_successful_dispatch() {
	let signer = RecordingSigner::new();
	for (outcome, expected) in [
		(Outcome::DispatchFailed, "failed"),
		(Outcome::MissingEvent, "event missing"),
		(Outcome::MissingExtrinsic, "extrinsic missing"),
	] {
		let mut node = EvmNode::new();
		node.outcomes.push_back(outcome);
		let (_, _, caller) = serve_contracts(node, GENESIS);
		let prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

		let result = block_on(caller.execute_and_watch(prepared));

		let actual = match result {
			Err(EvmContractError::DispatchFailed) => "failed",
			Err(EvmContractError::OutcomeMissing) => "extrinsic missing",
			Err(EvmContractError::Chain(failure))
				if failure.method() == "dispatch_outcome" &&
					failure.reason().contains("no dispatch event") =>
			{
				"event missing"
			},
			other => panic!("unexpected dispatch outcome: {other:?}"),
		};
		assert_eq!(actual, expected);
	}
}

#[test]
fn a_stopped_watch_is_not_reported_as_a_successful_contract_call() {
	let signer = RecordingSigner::new();
	for event in ["invalid", "dropped", "error"] {
		let mut node = EvmNode::new();
		node.outcomes.push_back(Outcome::Stopped(event));
		let (_, _, caller) = serve_contracts(node, GENESIS);
		let prepared = block_on(caller.dry_run(&signer, &CONTRACT, INPUT)).unwrap();

		let result = block_on(caller.execute_and_watch(prepared));

		let Err(EvmContractError::NotFinalized(actual)) = result else {
			panic!("unexpected stopped watch: {result:?}");
		};
		let expected = match event {
			"invalid" => WatchEvent::Invalid("scripted stop".to_string()),
			"dropped" => WatchEvent::Dropped("scripted stop".to_string()),
			_ => WatchEvent::Error("scripted stop".to_string()),
		};
		assert_eq!(actual, expected);
	}
}

#[test]
fn a_reverted_simulation_never_produces_a_transaction_to_submit() {
	let signer = RecordingSigner::new();
	let mut node = EvmNode::new();
	node.output = simulation_output(1);
	let (_, provider, caller) = serve_contracts(node, GENESIS);

	let result = block_on(caller.dry_run(&signer, &CONTRACT, INPUT));

	assert!(matches!(result, Err(EvmContractError::Reverted { .. })));
	assert_eq!(
		(
			method_count(&provider, "transactionWatch_v1_submitAndWatch"),
			signer.signatures.load(Ordering::SeqCst),
		),
		(0, 0),
	);
}

#[test]
fn runtime_rejections_and_malformed_simulations_produce_no_transaction() {
	let signer = RecordingSigner::new();
	for (output, expected) in [
		(simulation_result(Err(RawDispatchError(vec![0]))), ("dispatch", "0x00".to_string())),
		(vec![0xff], ("malformed", String::new())),
	] {
		let mut node = EvmNode::new();
		node.output = output;
		let (_, provider, caller) = serve_contracts(node, GENESIS);

		let result = block_on(caller.dry_run(&signer, &CONTRACT, INPUT));

		let actual = match result {
			Err(EvmContractError::Dispatch { detail }) => ("dispatch", detail),
			Err(EvmContractError::Malformed(_)) => ("malformed", String::new()),
			Err(other) => panic!("unexpected simulation failure: {other:?}"),
			Ok(_) => panic!("invalid simulation prepared a transaction"),
		};
		assert_eq!(
			(
				actual,
				method_count(&provider, "transactionWatch_v1_submitAndWatch"),
				signer.signatures.load(Ordering::SeqCst),
			),
			(expected, 0, 0),
		);
	}
}
