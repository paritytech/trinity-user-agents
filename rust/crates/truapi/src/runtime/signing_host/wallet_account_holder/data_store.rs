//! The `//datastore` account that owns a seed's installation records in the
//! `AccountDataStore` contract, and the key the records are encrypted under, as
//! the apps derive them.

use std::collections::HashSet;

use schnorrkel::Keypair;
use zeroize::Zeroizing;

use super::{WalletAccountHolder, WalletKeys};
use crate::{
	host_logic::{
		dotns_gateway::account_to_h160, keyed_hash, product_account::ProductAccountError,
		x25519_seal::AeadKey,
	},
	installation::InstallationId,
	runtime::AuthorityError,
};

/// The hard junction of the account that owns the records.
const DATA_STORE_JUNCTION: &str = "datastore";

/// What the record encryption key hashes, keyed with the account's secret.
const RECORD_KEY_CONTEXT: &[u8] = b"encryption";

impl WalletAccountHolder {
	/// The H160 the `AccountDataStore` contract keeps this seed's
	/// installation records under.
	pub fn data_store_owner(&self) -> Result<[u8; 20], AuthorityError> {
		self.with_installed_keys(|keys| Ok(keys.data_store_owner()?))
	}

	/// The distinct installations in `records` that were encrypted under this
	/// seed, skipping every other record.
	pub fn decrypt_installation_records(
		&self,
		records: &[Vec<u8>],
	) -> Result<HashSet<InstallationId>, AuthorityError> {
		self.with_installed_keys(|keys| {
			let key = keys.installation_record_encryption_key()?;
			Ok(decrypt_installation_records(&key, records))
		})
	}
}

impl WalletKeys {
	/// The H160 the contract keeps the seed's records under: the `//datastore`
	/// account mapped as pallet-revive maps it.
	pub fn data_store_owner(&self) -> Result<[u8; 20], ProductAccountError> {
		let public_key = self.data_store_account()?.public.to_bytes();
		Ok(account_to_h160(&public_key))
	}

	/// The key the seed's installation records are encrypted under.
	pub fn installation_record_encryption_key(&self) -> Result<AeadKey, ProductAccountError> {
		let secret = Zeroizing::new(self.data_store_account()?.secret.to_ed25519_bytes());
		Ok(record_encryption_key_from_secret(&secret))
	}

	/// The seed's `//datastore` account.
	fn data_store_account(&self) -> Result<Keypair, ProductAccountError> {
		self.entropy.derive_sr25519_hard_path(&[DATA_STORE_JUNCTION])
	}
}

/// BLAKE2b-256 of the context, keyed with the 64-byte secret in schnorrkel's
/// ed25519 form, the form polkadot-js and the Android SDK hold.
fn record_encryption_key_from_secret(secret: &[u8; 64]) -> AeadKey {
	let key = keyed_hash::blake2b_256_keyed(RECORD_KEY_CONTEXT, secret);
	AeadKey::from_bytes(Zeroizing::new(key))
}

/// The distinct installations in `records` that decrypt under `key`. A record
/// of another seed, or of another shape, is skipped, since nothing guarantees
/// every record in the list was written by an app of this seed.
fn decrypt_installation_records(key: &AeadKey, records: &[Vec<u8>]) -> HashSet<InstallationId> {
	records
		.iter()
		.filter_map(|record| decrypt_installation_record(key, record))
		.collect()
}

/// The installation in `record`, or `None` when it is not a record encrypted
/// under `key` around a 32-byte installation id.
fn decrypt_installation_record(key: &AeadKey, record: &[u8]) -> Option<InstallationId> {
	let plaintext = key.decrypt(record).ok()?;
	InstallationId::try_from(plaintext).ok()
}

#[cfg(test)]
mod tests;
