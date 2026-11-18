//! F4 · The installation registry: the `AccountDataStore` contract on Asset
//! Hub, which lists the sealed installation identifiers of a seed's
//! `//datastore` account (F4 invariant 4).
//!
//! The calls are encoded with the contract's own ABI, and the list belongs to
//! the account's contract address: the last 20 bytes of `keccak256` of its
//! account id, as the contract runtime maps an sr25519 account.

use std::sync::Arc;

use alloy_sol_types::SolCall;
use subxt::utils::H256;

use super::{
	CoinSecret, ledger,
	namespace::datastore_key,
	plugins::{PersonhoodSource, RegistryWriter, Seed},
	reconstruction::InstallationRegistry,
};
use crate::{
	chain::{ExtrinsicService, MortalExtrinsic},
	chain_runtime::ChainRuntime,
	evm_contract::EvmContractCaller,
	host_internal::extrinsic::Sr25519Signer,
	installation::{InstallationId, InstallationIdProvider},
	runtime::statement_allowance::{self, ChainClient, pgas, rpc::RpcClient},
	store::Db,
};
use truapi::latest::GenericError;

alloy_sol_types::sol! {
	/// The registry contract's two calls.
	interface AccountDataStore {
		function registerCoinageInstallation(bytes record) external;
		function getCoinageInstallations(address owner) external view returns (bytes[] memory);
	}
}

/// The input that appends `record` to the caller's list.
pub fn register_call(record: &[u8]) -> Vec<u8> {
	AccountDataStore::registerCoinageInstallationCall { record: record.to_vec().into() }.abi_encode()
}

/// The input that reads `owner`'s list.
pub fn installations_call(owner: [u8; 20]) -> Vec<u8> {
	AccountDataStore::getCoinageInstallationsCall { owner: owner.into() }.abi_encode()
}

/// The records in a read's output; none when the output does not decode,
/// which a caller must not take for an empty list.
pub fn decode_installations(output: &[u8]) -> Option<Vec<Vec<u8>>> {
	let records = AccountDataStore::getCoinageInstallationsCall::abi_decode_returns(output).ok()?;
	Some(records.into_iter().map(|record| record.to_vec()).collect())
}

/// The contract address of the sr25519 account `account`.
pub fn owner_address(account: [u8; 32]) -> [u8; 20] {
	let hash = alloy_primitives::keccak256(account);
	hash[12..].try_into().expect("a keccak hash has 32 bytes")
}

/// The registry read through the contract on the chain with `genesis`.
pub struct ContractRegistry {
	/// The chain connections.
	pub chain: ChainRuntime,
	/// Asset Hub's genesis.
	pub genesis: H256,
	/// The contract's address.
	pub contract: [u8; 20],
	/// The seed, whose `//datastore` account owns the list.
	pub seed: Arc<dyn Seed>,
}

#[async_trait::async_trait]
impl InstallationRegistry for ContractRegistry {
	async fn records(&self) -> Option<Vec<Vec<u8>>> {
		let (_, account) = datastore_key(&self.seed.entropy()?).ok()?;
		let connection = self.chain.legacy_connection(&self.genesis.0).await.ok()?;
		let mut rpc = connection.raw_rpc_client;
		let input = installations_call(owner_address(account));
		let output = EvmContractCaller::read(&mut rpc, &self.contract, &input).await.ok()?;
		decode_installations(&output)
	}
}

/// The appends to the `AccountDataStore` contract on Asset Hub, paid in PGAS
/// by the `//datastore` account, as the shipped wallets pay them.
pub struct ContractRegistryWriter {
	/// The chain connections.
	pub chain: ChainRuntime,
	/// Builds extrinsics.
	pub extrinsics: Arc<ExtrinsicService>,
	/// Asset Hub's genesis.
	pub asset_hub_genesis: H256,
	/// The People chain's genesis, where the person's ring lives.
	pub people_genesis: H256,
	/// The contract's address.
	pub contract: [u8; 20],
	/// The person whose ring proof claims the PGAS.
	pub personhood: Arc<dyn PersonhoodSource>,
}

#[async_trait::async_trait]
impl RegistryWriter for ContractRegistryWriter {
	async fn append(&self, datastore: &CoinSecret, record: &[u8]) -> Result<MortalExtrinsic, String> {
		let signer = Sr25519Signer::from_secret_bytes(&datastore.0).map_err(|error| format!("{error:?}"))?;
		self.fund(subxt::tx::Signer::account_id(&signer).0).await?;
		let caller = EvmContractCaller::new(self.chain.clone(), self.extrinsics.clone(), self.asset_hub_genesis);
		let prepared = caller.dry_run(&signer, &self.contract, &register_call(record)).await.map_err(|error| error.to_string())?;
		prepared.sign().await.map_err(|error| error.to_string())
	}
}

impl ContractRegistryWriter {
	/// Claims PGAS for `account` unless it holds at least one claim's worth,
	/// which covers the append's fee and storage deposit.
	async fn fund(&self, account: [u8; 32]) -> Result<(), String> {
		let failed = |error: statement_allowance::StatementAllowanceError| error.to_string();
		let asset_hub = self.client("coinage registry", self.asset_hub_genesis).await?;
		let asset_hub_metadata = statement_allowance::fetch_metadata(asset_hub.rpc()).await.map_err(failed)?;
		if pgas::holds_a_full_claim(asset_hub.rpc(), &asset_hub_metadata, &account).await.map_err(failed)? {
			return Ok(());
		}
		let (signer, ring, _) = self.personhood.person().await.ok_or("the person's ring is not available")?;
		let people = self.client("coinage registry ring", self.people_genesis).await?;
		let people_metadata = statement_allowance::fetch_metadata(people.rpc()).await.map_err(failed)?;
		let network_suffix = statement_allowance::slot::read_network_suffix(asset_hub.rpc()).await.map_err(failed)?;
		pgas::claim_pgas(pgas::PgasClaim {
			asset_hub_rpc: asset_hub.rpc(),
			asset_hub_metadata: &asset_hub_metadata,
			asset_hub_target: asset_hub.extrinsic_target(&self.extrinsics),
			people_rpc: people.rpc(),
			people_metadata: &people_metadata,
			signer: signer.as_ref(),
			network_suffix: &network_suffix,
			target: &account,
			ring: &ring,
		})
		.await
		.map_err(failed)?;
		Ok(())
	}

	async fn client(&self, method: &'static str, genesis: H256) -> Result<ChainClient, String> {
		let rpc = self.chain.rpc_client(method, &genesis.0).await.map_err(|error| error.to_string())?;
		Ok(ChainClient::new(RpcClient::new(subxt_rpcs::RpcClient::new(rpc)), genesis.0))
	}
}

/// The registry of a host that runs none: its read always fails, so a
/// reconstruction scans only the installations it knows.
pub struct NoRegistry;

#[async_trait::async_trait]
impl InstallationRegistry for NoRegistry {
	async fn records(&self) -> Option<Vec<Vec<u8>>> {
		None
	}
}

/// The installation the coinage layer minted, handed to the rest of the core
/// (G3 invariant 3).
pub struct StoredInstallation(pub Db);

#[async_trait::async_trait]
impl InstallationIdProvider for StoredInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		let current = self.0.read(ledger::current_installation).await;
		let failure = |reason: String| GenericError { reason };
		current
			.map_err(|error| failure(format!("the coinage store failed: {error}")))?
			.ok_or_else(|| failure("the coinage layer has not minted its installation yet".into()))
	}
}

#[cfg(test)]
mod tests;
