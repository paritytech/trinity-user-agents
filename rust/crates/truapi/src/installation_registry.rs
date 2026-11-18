//! Reads the installations a seed registered in the `AccountDataStore`
//! contract on Asset Hub.
//!
//! The apps register each installation, encrypted under a key only the seed
//! can derive, before handing it out. [`AccountDataStoreContract`] reads the
//! encrypted records an owner keeps, and [`RegisteredInstallations`] reads
//! the seed's records through it and decrypts them through
//! [`DataStoreKeys`], which keeps the seed in the signing host.

use std::{
	collections::HashSet,
	sync::{Arc, Weak},
};

use alloy_sol_types::SolCall;
use subxt::utils::H256;

use crate::{
	chain::{EvmContractCaller, EvmContractError, RpcClientAtBlock, RuntimeApiCaller},
	chain_runtime::{ChainAccess, RuntimeFailure},
	installation::InstallationId,
	runtime::AuthorityError,
};

alloy_sol_types::sol! {
	function getCoinageInstallations(address owner) external view returns (bytes[] memory);
}

/// Reads the seed's installation records with keys derived from its
/// `//datastore` account, without handing out the seed.
pub trait DataStoreKeys: Send + Sync {
	/// The H160 the contract keeps the records under. Fails without a
	/// session.
	fn data_store_owner(&self) -> Result<[u8; 20], AuthorityError>;

	/// The distinct installations in `records` that were encrypted under this
	/// seed, skipping every other record. Fails without a session.
	fn decrypt_installation_records(
		&self,
		records: &[Vec<u8>],
	) -> Result<HashSet<InstallationId>, AuthorityError>;
}

/// Why the registered installations could not be read.
#[derive(Debug, thiserror::Error)]
pub enum RegistryReadError {
	/// The seed's keys are not available.
	#[error(transparent)]
	Keys(#[from] AuthorityError),
	/// The chain could not be reached.
	#[error(transparent)]
	Connection(#[from] RuntimeFailure),
	/// The contract read failed.
	#[error("reading the installation registry failed: {0}")]
	Read(#[from] EvmContractError),
	/// The contract returned something other than a list of records.
	#[error("the installation registry returned malformed data, expected an ABI `bytes[]`: {0}")]
	Malformed(alloy_sol_types::Error),
}

/// Reads the encrypted installation records an owner keeps in the registry.
#[async_trait::async_trait]
pub trait InstallationRegistryChain: Send + Sync {
	/// The records `owner` keeps as of the block `at`, in the order the
	/// contract lists them. Fails when the read fails or returns malformed
	/// data.
	async fn encrypted_records(
		&self,
		owner: [u8; 20],
		at: H256,
	) -> Result<Vec<Vec<u8>>, RegistryReadError>;
}

/// The `AccountDataStore` contract on one chain.
pub struct AccountDataStoreContract {
	chain: ChainAccess,
	contract: [u8; 20],
}

impl AccountDataStoreContract {
	/// The contract at `contract` on `chain`.
	pub fn new(chain: ChainAccess, contract: [u8; 20]) -> Self {
		Self { chain, contract }
	}
}

#[async_trait::async_trait]
impl InstallationRegistryChain for AccountDataStoreContract {
	async fn encrypted_records(
		&self,
		owner: [u8; 20],
		at: H256,
	) -> Result<Vec<Vec<u8>>, RegistryReadError> {
		let connection = self.chain.legacy_connection().await?;
		let mut node = RpcClientAtBlock::new(connection.raw_rpc_client, at);
		read_encrypted_records(&mut node, &self.contract, owner).await
	}
}

/// The records `owner` keeps in `contract`, read through `runtime`.
async fn read_encrypted_records(
	runtime: &mut impl RuntimeApiCaller,
	contract: &[u8; 20],
	owner: [u8; 20],
) -> Result<Vec<Vec<u8>>, RegistryReadError> {
	let call = getCoinageInstallationsCall { owner: owner.into() }.abi_encode();
	let output = EvmContractCaller::read(runtime, contract, &call).await?;
	let records = getCoinageInstallationsCall::abi_decode_returns_validate(&output)
		.map_err(RegistryReadError::Malformed)?;
	Ok(records.into_iter().map(Vec::from).collect())
}

/// The installations the session's seed registered.
pub struct RegisteredInstallations {
	chain: Arc<dyn InstallationRegistryChain>,
	keys: Weak<dyn DataStoreKeys>,
}

impl RegisteredInstallations {
	/// The seed's installations in the registry `chain` reads, decrypted with
	/// `keys` while they are alive.
	pub fn new(chain: Arc<dyn InstallationRegistryChain>, keys: Weak<dyn DataStoreKeys>) -> Self {
		Self { chain, keys }
	}

	/// Every installation registered for the session's seed as of the block
	/// `at`. Records the seed cannot decrypt are skipped. Fails without a
	/// session, or when the read fails or returns malformed data.
	pub async fn read_installations(
		&self,
		at: H256,
	) -> Result<HashSet<InstallationId>, RegistryReadError> {
		let Some(keys) = self.keys.upgrade() else {
			let reason = "the data store keys were dropped".to_string();
			return Err(AuthorityError::Unavailable { reason }.into());
		};
		let owner = keys.data_store_owner()?;
		let records = self.chain.encrypted_records(owner, at).await?;
		Ok(keys.decrypt_installation_records(&records)?)
	}
}

#[cfg(test)]
mod tests;
