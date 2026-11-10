//! Bulletin store transactions, and reading stored blobs back by CID.

mod cid;
mod read;

use subxt::{ext::scale_encode::EncodeAsType, utils::H256};

use crate::{
	chain::{ExtrinsicBuildError, ExtrinsicService, MortalExtrinsic},
	host_internal::extrinsic::Sr25519Signer,
	runtime::BulletinAllowanceKey,
};

pub use cid::{BulletinCid, BulletinCidError};
pub use read::{BitswapError, bitswap_get};

/// Pallet the store transaction targets.
pub const STORE_PALLET_NAME: &str = "TransactionStorage";
/// Call name within [`STORE_PALLET_NAME`].
pub const STORE_CALL_NAME: &str = "store";

/// Preimage key: blake2b-256 of the raw preimage bytes.
pub fn preimage_key(value: &[u8]) -> [u8; 32] {
	sp_crypto_hashing::blake2_256(value)
}

/// Build the signed store transaction for `data` on the Bulletin chain
/// `genesis`, born at its finalized head.
pub async fn build_signed_store_transaction(
	extrinsics: &ExtrinsicService,
	genesis: H256,
	signer: &Sr25519Signer,
	data: &[u8],
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	extrinsics
		.build_mortal_extrinsic(genesis)
		.with_call_args(STORE_PALLET_NAME, STORE_CALL_NAME, StoreCallArgs { data })
		.with_signer(signer)
		.build()
		.await
}

/// The only [`BulletinAllowanceKey`] -> signer conversion in the crate. The
/// returned signer is a transient per-call value; callers must not store it.
pub fn allowance_signer(allowance: &BulletinAllowanceKey) -> Result<Sr25519Signer, String> {
	Sr25519Signer::from_secret_bytes(allowance.as_secret_bytes())
		.map_err(|reason| format!("invalid bulletin allowance key: {reason}"))
}

/// `TransactionStorage.store` arguments.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct StoreCallArgs<'a> {
	/// The preimage to store.
	data: &'a [u8],
}

#[cfg(test)]
mod tests;
