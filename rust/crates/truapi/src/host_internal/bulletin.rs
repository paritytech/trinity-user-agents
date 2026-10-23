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
mod tests {
	use std::sync::Arc;

	use futures::executor::block_on;
	use parity_scale_codec::Decode;
	use subxt::{
		ext::{
			frame_decode::extrinsics::encode_call_data,
			frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed, v14},
		},
		metadata::Metadata,
	};

	use super::*;
	use crate::chain::{Node, V4OnlyHost, serve};

	const GENESIS: H256 = H256([0xab; 32]);

	fn allowance_fixture() -> BulletinAllowanceKey {
		let secret = hex::decode(
			"0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
             573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
		)
		.unwrap();
		BulletinAllowanceKey::from_secret_bytes(secret).unwrap()
	}

	fn signer_fixture() -> Sr25519Signer {
		allowance_signer(&allowance_fixture()).unwrap()
	}

	/// A service over a scripted node serving the Bulletin fixture runtime.
	fn bulletin_service() -> ExtrinsicService {
		let (_node, _provider, chain) = serve(Node::new(4, 6));
		ExtrinsicService::over_chain(chain, Arc::new(V4OnlyHost))
	}

	/// Decode the fixture metadata down to its mutable v14 representation.
	fn bulletin_metadata_v14() -> v14::RuntimeMetadataV14 {
		let prefixed = RuntimeMetadataPrefixed::decode(
			&mut &crate::host_internal::extrinsic::tests::BULLETIN_METADATA_BYTES[..],
		)
		.unwrap();
		match prefixed.1 {
			RuntimeMetadata::V14(metadata) => metadata,
			other => panic!("expected v14 fixture metadata, got {other:?}"),
		}
	}

	fn metadata_from_v14(metadata: v14::RuntimeMetadataV14) -> Metadata {
		let prefixed =
			RuntimeMetadataPrefixed(u32::from_le_bytes(*b"meta"), RuntimeMetadata::V14(metadata));
		Metadata::try_from(prefixed).unwrap()
	}

	#[test]
	fn preimage_key_is_blake2b_256() {
		assert_eq!(
			hex::encode(preimage_key(b"")),
			"0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8"
		);
	}

	/// A blob stored on Paseo Bulletin: `bitswap_v1_get` with this CID returns the bytes whose
	/// blake2b-256 is the key.
	#[test]
	fn preimage_cid_is_cidv1_raw_blake2b_256_in_base32() {
		let key: [u8; 32] =
			hex::decode("0bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c")
				.unwrap()
				.try_into()
				.unwrap();
		assert_eq!(
			preimage_cid(&key),
			"bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy"
		);
	}

	/// If the runtime's `store` stops taking bytes, the call is refused at
	/// encoding rather than sent with a shape the node rejects.
	#[test]
	fn rejects_mutated_store_argument_type() {
		// Point the store call's `data` field at a non-byte type: the
		// CheckSpecVersion additional (u32) borrowed from the extension list.
		let mut metadata = bulletin_metadata_v14();
		let u32_type = metadata
			.extrinsic
			.signed_extensions
			.iter()
			.find(|extension| extension.identifier == "CheckSpecVersion")
			.unwrap()
			.additional_signed;
		let calls_type_id = metadata
			.pallets
			.iter()
			.find(|pallet| pallet.name == STORE_PALLET_NAME)
			.unwrap()
			.calls
			.as_ref()
			.unwrap()
			.ty
			.id;
		let calls_type = metadata.types.types.iter_mut().find(|ty| ty.id == calls_type_id).unwrap();
		let scale_info::TypeDef::Variant(variants) = &mut calls_type.ty.type_def else {
			panic!("calls type is not a variant");
		};
		let store = variants
			.variants
			.iter_mut()
			.find(|variant| variant.name == STORE_CALL_NAME)
			.unwrap();
		store.fields[0].ty = u32_type;
		let metadata = metadata_from_v14(metadata);
		let args = StoreCallArgs { data: &[1, 2, 3] };

		let error = encode_call_data(
			STORE_PALLET_NAME,
			STORE_CALL_NAME,
			&args,
			&metadata,
			metadata.types(),
		)
		.unwrap_err();

		assert!(error.to_string().contains("Error at data"), "{error}");
	}

	#[test]
	fn builds_large_preimage_without_pathological_cost() {
		// Keep a generous bound around the call encoding so a future library
		// change cannot make large preimages pathological.
		let data = vec![0x5au8; 8 * 1024 * 1024];
		let service = bulletin_service();
		let signer = signer_fixture();
		let start = std::time::Instant::now();

		let built =
			block_on(build_signed_store_transaction(&service, GENESIS, &signer, &data)).unwrap();

		let elapsed = start.elapsed();
		assert!(built.extrinsic.bytes().len() > data.len());
		assert!(
			elapsed < std::time::Duration::from_secs(5),
			"building an 8 MiB store extrinsic took {elapsed:?}"
		);
	}

	#[test]
	fn rejects_secret_of_wrong_shape() {
		let error =
			allowance_signer(&BulletinAllowanceKey::from_secret_bytes(vec![0xff; 64]).unwrap())
				.unwrap_err();
		assert!(error.contains("invalid bulletin allowance key"), "{error}");
	}
}
