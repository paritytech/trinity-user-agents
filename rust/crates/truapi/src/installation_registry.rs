//! Reads the installations a seed registered in the `AccountDataStore`
//! contract on Asset Hub.
//!
//! The apps register each installation, sealed under a key only the seed can
//! derive, before handing it out. [`RegisteredInstallations::read_installations`]
//! reads the seed's list and opens it through [`DataStoreKeys`], which keeps
//! the seed in the signing host.

use std::sync::Weak;

use alloy_sol_types::SolCall;

use crate::{
	chain::{EvmContractCaller, EvmContractError, RuntimeApiCaller},
	installation::InstallationId,
	runtime::AuthorityError,
};

alloy_sol_types::sol! {
	function getCoinageInstallations(address owner) external view returns (bytes[] memory);
}

/// The seed's `//datastore` account, which owns its installation records.
pub trait DataStoreKeys: Send + Sync {
	/// The H160 the contract keeps the records under. Fails without a
	/// session.
	fn data_store_owner(&self) -> Result<[u8; 20], AuthorityError>;

	/// The distinct installations in `records` that were sealed under this
	/// seed, skipping every other record. Fails without a session.
	fn open_installation_records(
		&self,
		records: &[Vec<u8>],
	) -> Result<Vec<InstallationId>, AuthorityError>;
}

/// Why the registered installations could not be read.
#[derive(Debug, thiserror::Error)]
pub enum RegistryReadError {
	/// The seed's keys are not available.
	#[error(transparent)]
	Keys(#[from] AuthorityError),
	/// The contract read failed.
	#[error("reading the installation registry failed: {0}")]
	Read(#[from] EvmContractError),
	/// The contract returned something other than a list of records.
	#[error("the installation registry returned malformed data: {0}")]
	Malformed(alloy_sol_types::Error),
}

/// The installations a seed registered in one `AccountDataStore` contract.
pub struct RegisteredInstallations {
	contract: [u8; 20],
	keys: Weak<dyn DataStoreKeys>,
}

impl RegisteredInstallations {
	/// The registry at `contract`, opened with `keys` while they are alive.
	pub fn new(contract: [u8; 20], keys: Weak<dyn DataStoreKeys>) -> Self {
		Self { contract, keys }
	}

	/// Every installation registered for the session's seed, read through
	/// `runtime` and in the order the contract lists them. Records the seed
	/// cannot open are skipped. Fails without a session, or when the read
	/// fails or returns malformed data.
	pub async fn read_installations(
		&self,
		runtime: &mut impl RuntimeApiCaller,
	) -> Result<Vec<InstallationId>, RegistryReadError> {
		let Some(keys) = self.keys.upgrade() else {
			let reason = "the data store keys were dropped".to_string();
			return Err(AuthorityError::Unavailable { reason }.into());
		};
		let owner = keys.data_store_owner()?.into();
		let call = getCoinageInstallationsCall { owner }.abi_encode();
		let output = EvmContractCaller::read(runtime, &self.contract, &call).await?;
		let records = getCoinageInstallationsCall::abi_decode_returns_validate(&output)
			.map_err(RegistryReadError::Malformed)?;
		let records: Vec<Vec<u8>> = records.into_iter().map(Vec::from).collect();
		Ok(keys.open_installation_records(&records)?)
	}
}

#[cfg(test)]
mod tests;
