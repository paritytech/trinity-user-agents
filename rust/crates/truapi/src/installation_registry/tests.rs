use std::sync::{Arc, Mutex};

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

	fn open_installation_records(
		&self,
		records: &[Vec<u8>],
	) -> Result<Vec<InstallationId>, AuthorityError> {
		Ok(records.iter().filter_map(|record| record.clone().try_into().ok()).collect())
	}
}

/// A node that answers one `ReviveApi_call` and keeps its input.
struct AnsweringNode {
	answer: Option<Result<Vec<u8>, subxt::Error>>,
	inputs: Arc<Mutex<Vec<Vec<u8>>>>,
}

#[async_trait::async_trait]
impl RuntimeApiCaller for AnsweringNode {
	async fn call(&mut self, function: &str, input: &[u8]) -> Result<Vec<u8>, subxt::Error> {
		assert_eq!(function, "ReviveApi_call");
		self.inputs.lock().unwrap().push(input.to_vec());
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

fn read(registry: &RegisteredInstallations, answer: Vec<u8>) -> Result<Vec<InstallationId>, RegistryReadError> {
	let mut node = AnsweringNode { answer: Some(Ok(answer)), inputs: Arc::default() };
	futures::executor::block_on(registry.read_installations(&mut node))
}

/// The core asks for the seed's list exactly as the apps do, and gets back
/// what opens under the seed.
#[test]
fn the_registry_asks_for_the_owners_list_and_returns_what_opens() {
	let keys: Arc<dyn DataStoreKeys> = Arc::new(PlainRecords);
	let registry = RegisteredInstallations::new(CONTRACT, Arc::downgrade(&keys));
	let records: Vec<Bytes> = vec![vec![0x5a; 32].into(), vec![1, 2].into()];
	let output = getCoinageInstallationsCall::abi_encode_returns(&records);
	let inputs = Arc::default();
	let mut node = AnsweringNode { answer: Some(Ok(returned(output))), inputs: Arc::clone(&inputs) };

	let installations = futures::executor::block_on(registry.read_installations(&mut node));

	assert_eq!(installations.unwrap(), [InstallationId([0x5a; 32])]);
	let call = hex::decode(GET_CALL).unwrap();
	assert!(inputs.lock().unwrap()[0].ends_with(&call.encode()));
}

/// Data that is not a list of records must not read as an empty registry, or
/// recovery would mark every earlier installation as having nothing.
#[test]
fn malformed_data_fails_the_read() {
	let keys: Arc<dyn DataStoreKeys> = Arc::new(PlainRecords);
	let registry = RegisteredInstallations::new(CONTRACT, Arc::downgrade(&keys));

	let installations = read(&registry, returned(vec![0xff; 3]));

	assert!(matches!(installations, Err(RegistryReadError::Malformed(_))));
}

/// Without the signing host there is no seed to read the list for.
#[test]
fn the_read_fails_once_the_keys_are_dropped() {
	let registry = RegisteredInstallations::new(CONTRACT, Weak::<PlainRecords>::new());

	let installations = read(&registry, returned(Vec::new()));

	assert!(matches!(installations, Err(RegistryReadError::Keys(AuthorityError::Unavailable { .. }))));
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
	use crate::chain::{RpcClientAtBlock, live_asset_hub, live_asset_hub_genesis};

	let contract = std::env::var("LIVE_ACCOUNT_DATA_STORE")
		.unwrap_or_else(|_| "0x58c9963308c7735cf72ab633e8a9d892f5ab9e05".into());
	let contract: [u8; 20] =
		hex::decode(contract.trim_start_matches("0x")).unwrap().try_into().unwrap();
	let genesis = live_asset_hub_genesis();
	let connection = live_asset_hub(genesis).legacy_connection(genesis.as_bytes()).await.unwrap();
	let finalized = connection.methods.chain_get_finalized_head().await.unwrap();
	let mut node = RpcClientAtBlock::new(connection.raw_rpc_client, finalized);
	let keys: Arc<dyn DataStoreKeys> = Arc::new(PlainRecords);
	let registry = RegisteredInstallations::new(contract, Arc::downgrade(&keys));

	let installations = registry.read_installations(&mut node).await;

	assert!(installations.is_ok(), "{installations:?}");
}
