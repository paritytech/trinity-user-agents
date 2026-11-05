//! Bulletin store transaction construction and signing.

use subxt::{ext::scale_encode::EncodeAsType, utils::H256};

use crate::{
	chain::{ExtrinsicBuildError, ExtrinsicService, MortalExtrinsic},
	host_internal::extrinsic::Sr25519Signer,
	runtime::BulletinAllowanceKey,
};

/// Pallet the store transaction targets.
pub const STORE_PALLET_NAME: &str = "TransactionStorage";
/// Call name within [`STORE_PALLET_NAME`].
pub const STORE_CALL_NAME: &str = "store";

/// Preimage key: blake2b-256 of the raw preimage bytes.
pub fn preimage_key(value: &[u8]) -> [u8; 32] {
	sp_crypto_hashing::blake2_256(value)
}

/// The CID transaction storage serves a preimage under, as a Bulletin node's
/// `bitswap_v1_get` takes it: CIDv1, the `raw` codec and the blake2b-256 multihash of the
/// content, which is the preimage key itself, in lower-case base32 with the multibase `b`
/// prefix.
pub fn preimage_cid(key: &[u8; 32]) -> String {
	const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
	// CIDv1, raw, then the multihash: blake2b-256 (0xb220 as a varint) of 32 bytes.
	let mut bytes = vec![0x01, 0x55, 0xa0, 0xe4, 0x02, 0x20];
	bytes.extend_from_slice(key);
	let mut cid = String::from("b");
	let (mut buffer, mut bits) = (0u32, 0u32);
	for byte in bytes {
		buffer = (buffer << 8) | u32::from(byte);
		bits += 8;
		while bits >= 5 {
			cid.push(char::from(ALPHABET[((buffer >> (bits - 5)) & 31) as usize]));
			bits -= 5;
		}
	}
	if bits > 0 {
		cid.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
	}
	cid
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
