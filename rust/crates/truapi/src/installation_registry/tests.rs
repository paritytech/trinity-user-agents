use alloy_primitives::Bytes;
use parity_scale_codec::Encode;

use super::*;
use crate::chain::{ContractResult, ExecReturnValue, RawDispatchError, StorageDeposit, Weight};

/// The owner and call the Android app's ABI coder test pins.
const OWNER: [u8; 20] = [0x11; 20];
const GET_CALL: &str = "740204c60000000000000000000000001111111111111111111111111111111111111111";

const CONTRACT: [u8; 20] = [0x58; 20];

/// Keys whose records are the 32 installation id bytes themselves.
struct PlainRecords;

impl DataStoreKeys for PlainRecords {
	fn data_store_owner(&self) -> Result<[u8; 20], AuthorityError> {
		Ok(OWNER)
	}

	fn decrypt_installation_records(
		&self,
		records: &[Vec<u8>],
	) -> Result<HashSet<InstallationId>, AuthorityError> {
		Ok(records.iter().filter_map(|record| record.clone().try_into().ok()).collect())
	}
}

/// A node that answers one `ReviveApi_call` and keeps its input.
struct AnsweringNode {
	answer: Option<Result<Vec<u8>, subxt::Error>>,
	inputs: Vec<Vec<u8>>,
}

impl AnsweringNode {
	fn returning(data: Vec<u8>) -> Self {
		Self { answer: Some(Ok(returned(data))), inputs: Vec::new() }
	}
}

#[async_trait::async_trait]
impl RuntimeApiCaller for AnsweringNode {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error> {
		assert_eq!(function, "ReviveApi_call");
		self.inputs.push(input.to_vec());
		self.answer.take().expect("one call is answered")
	}

	async fn call_batch(
		&mut self,
		_function: &str,
		_inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, subxt::Error>> {
		unreachable!("the registry reads with one call")
	}
}

fn returned(data: Vec<u8>) -> Vec<u8> {
	ContractResult {
		_weight_consumed: Weight { ref_time: 1, proof_size: 1 },
		weight_required: Weight { ref_time: 1, proof_size: 1 },
		storage_deposit: StorageDeposit::Charge(0),
		max_storage_deposit: StorageDeposit::Charge(0),
		_gas_consumed: 1,
		result: Ok::<_, RawDispatchError>(ExecReturnValue { flags: 0, data }),
	}
	.encode()
}

/// The core asks for the owner's list exactly as the apps do, and gets back
/// every record in it.
#[test]
fn the_contract_is_asked_for_the_owners_list_as_the_apps_ask() {
	let records: Vec<Bytes> = vec![vec![0x5a; 32].into(), vec![1, 2].into()];
	let mut node =
		AnsweringNode::returning(getCoinageInstallationsCall::abi_encode_returns(&records));

	let read = futures::executor::block_on(read_encrypted_records(&mut node, &CONTRACT, OWNER));

	assert_eq!(read.unwrap(), [vec![0x5a; 32], vec![1, 2]]);
	let call = hex::decode(GET_CALL).unwrap();
	assert!(node.inputs[0].ends_with(&call.encode()));
}

/// Data that is not a list of records must not read as an empty registry, or
/// recovery would mark every earlier installation as having nothing.
#[test]
fn malformed_data_fails_the_read() {
	let mut node = AnsweringNode::returning(vec![0xff; 3]);

	let read = futures::executor::block_on(read_encrypted_records(&mut node, &CONTRACT, OWNER));

	assert!(matches!(read, Err(RegistryReadError::Malformed(_))));
}

/// The seed's list is read for the seed's own owner, and only what decrypts
/// under the seed comes back.
#[test]
fn only_the_seeds_own_records_come_back() {
	let records: Vec<Bytes> = vec![vec![0x5a; 32].into(), vec![1, 2].into()];
	let mut node =
		AnsweringNode::returning(getCoinageInstallationsCall::abi_encode_returns(&records));

	let installations =
		futures::executor::block_on(read_seed_installations(&mut node, &CONTRACT, &PlainRecords));

	assert_eq!(installations.unwrap(), HashSet::from([InstallationId([0x5a; 32])]));
	let call = hex::decode(GET_CALL).unwrap();
	assert!(node.inputs[0].ends_with(&call.encode()));
}

/// Without the signing host there is no seed to read the list for.
#[test]
fn the_keys_are_unavailable_once_dropped() {
	let dropped: Weak<dyn DataStoreKeys> = Weak::<PlainRecords>::new();

	let keys = upgrade_data_store_keys(&dropped);

	assert!(matches!(keys, Err(AuthorityError::Unavailable { .. })));
}

/// The contract the apps register against answers the read and returns a
/// list. Run against pcf-devnet's Asset Hub with
/// `LIVE_ASSET_HUB_WS=wss://asset-hub-paseo-rpc.n.dwellir.com
/// LIVE_ASSET_HUB_GENESIS=0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2
/// cargo test -p truapi --lib installation_registry -- --ignored`.
/// `LIVE_ACCOUNT_DATA_STORE` names another contract than pcf-devnet's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Asset Hub with an AccountDataStore contract"]
async fn the_live_registry_answers_at_the_finalized_block() {
	use crate::chain::{ChainHeads, live_asset_hub, live_asset_hub_genesis};

	let contract = std::env::var("LIVE_ACCOUNT_DATA_STORE")
		.unwrap_or_else(|_| "0x58c9963308c7735cf72ab633e8a9d892f5ab9e05".into());
	let contract: [u8; 20] =
		hex::decode(contract.trim_start_matches("0x")).unwrap().try_into().unwrap();
	let genesis = live_asset_hub_genesis();
	let runtime = live_asset_hub(genesis);
	let finalized = ChainHeads::finalized_block(&runtime, genesis).await.unwrap();
	let keys: Arc<dyn DataStoreKeys> = Arc::new(PlainRecords);
	let chain = ChainAccess::new(runtime, genesis);
	let registry = RegisteredInstallations::new(chain, contract, Arc::downgrade(&keys));

	let installations = registry.read_installations(finalized.hash).await;

	assert!(installations.is_ok(), "{installations:?}");
}
