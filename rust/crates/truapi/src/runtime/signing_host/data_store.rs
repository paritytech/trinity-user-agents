//! The `//datastore` account that owns a seed's installation records in the
//! `AccountDataStore` contract, and the key the records are sealed under, as
//! the apps derive them.

use std::collections::HashSet;

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

/// A record's length: a 12-byte nonce, the 32-byte installation id, and a
/// 16-byte tag.
const RECORD_LEN: usize = 12 + 32 + 16;

/// The H160 the contract keeps the seed's records under: the `//datastore`
/// account mapped as pallet-revive maps it.
pub fn data_store_owner(entropy: &[u8]) -> Result<[u8; 20], ProductAccountError> {
	let account = derive_sr25519_hard_path(entropy, &[DATA_STORE_JUNCTION])?;
	Ok(account_to_h160(&account.public.to_bytes()))
}

/// The key the seed's installation records are sealed under.
pub fn installation_record_key(entropy: &[u8]) -> Result<AeadKey, ProductAccountError> {
	let account = derive_sr25519_hard_path(entropy, &[DATA_STORE_JUNCTION])?;
	let secret = Zeroizing::new(account.secret.to_ed25519_bytes());
	Ok(record_key_from_secret(&secret))
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
/// is skipped, since nothing guarantees every record in the list was written
/// by an app of this seed.
pub fn open_installation_records(key: &AeadKey, records: &[Vec<u8>]) -> Vec<InstallationId> {
	let mut seen = HashSet::new();
	records
		.iter()
		.filter_map(|record| open_installation_record(key, record))
		.filter(|installation| seen.insert(*installation))
		.collect()
}

/// The installation in `record`, or `None` when it is not a record sealed
/// under `key`.
fn open_installation_record(key: &AeadKey, record: &[u8]) -> Option<InstallationId> {
	if record.len() != RECORD_LEN {
		return None;
	}
	let plaintext = key.decrypt(record).ok()?;
	Some(InstallationId(plaintext.try_into().ok()?))
}

#[cfg(test)]
mod tests;
