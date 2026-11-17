//! The `//datastore` account that owns a seed's installation records in the
//! `AccountDataStore` contract, and the key the records are sealed under, as
//! the apps derive them.

use std::collections::HashSet;

use schnorrkel::Keypair;
use zeroize::Zeroizing;

use crate::{
	host_logic::{
		dotns_gateway::account_to_h160,
		product_account::{ProductAccountError, derive_sr25519_hard_path},
		x25519_seal::AeadKey,
	},
	installation::InstallationId,
};

/// The hard junction of the account that owns the records.
const DATA_STORE_JUNCTION: &str = "datastore";

/// What the record key hashes, keyed with the account's secret.
const RECORD_KEY_CONTEXT: &[u8] = b"encryption";

/// The H160 the contract keeps the seed's records under: the `//datastore`
/// account mapped as pallet-revive maps it.
pub fn data_store_owner(entropy: &[u8]) -> Result<[u8; 20], ProductAccountError> {
	let public_key = data_store_account(entropy)?.public.to_bytes();
	Ok(account_to_h160(&public_key))
}

/// The key the seed's installation records are sealed under.
pub fn installation_record_key(entropy: &[u8]) -> Result<AeadKey, ProductAccountError> {
	let secret = Zeroizing::new(data_store_account(entropy)?.secret.to_ed25519_bytes());
	Ok(record_key_from_secret(&secret))
}

/// The seed's `//datastore` account.
fn data_store_account(entropy: &[u8]) -> Result<Keypair, ProductAccountError> {
	derive_sr25519_hard_path(entropy, &[DATA_STORE_JUNCTION])
}

/// BLAKE2b-256 of the context, keyed with the 64-byte secret in schnorrkel's
/// ed25519 form, the form polkadot-js and the Android SDK hold.
fn record_key_from_secret(secret: &[u8; 64]) -> AeadKey {
	let hash = blake2b_simd::Params::new().hash_length(32).key(secret).hash(RECORD_KEY_CONTEXT);
	let key: [u8; 32] = hash.as_bytes().try_into().expect("the hash length is 32 bytes; qed");
	AeadKey::from_bytes(Zeroizing::new(key))
}

/// The distinct installations in `records` that open under `key`, in the
/// order they first appear. A record of another seed, or of another shape,
/// is skipped and counted in a debug line, since nothing guarantees every
/// record in the list was written by an app of this seed.
pub fn open_installation_records(key: &AeadKey, records: &[Vec<u8>]) -> Vec<InstallationId> {
	let mut seen = HashSet::new();
	let opened: Vec<InstallationId> = records
		.iter()
		.filter_map(|record| open_installation_record(key, record))
		.filter(|installation| seen.insert(*installation))
		.collect();
	let skipped = records.len() - opened.len();
	tracing::debug!(skipped, records = records.len(), "installation records skipped");
	opened
}

/// The installation in `record`, or `None` when it is not a record sealed
/// under `key` around a 32-byte installation id.
fn open_installation_record(key: &AeadKey, record: &[u8]) -> Option<InstallationId> {
	let plaintext: [u8; 32] = key.decrypt(record).ok()?.try_into().ok()?;
	Some(InstallationId(plaintext))
}

#[cfg(test)]
mod tests;
