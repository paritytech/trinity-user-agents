//! Extrinsic signing preimages assembled from pre-encoded payload fields.
//!
//! Signing hosts receive the pre-encoded fields of [`HostSignPayloadData`],
//! so no chain metadata is needed for the standard Substrate extensions. The
//! payload's `signed_extensions` list supplies runtime order; extensions whose
//! bytes cannot be represented by [`HostSignPayloadData`] are rejected instead
//! of being silently omitted. Preimages longer than 256 bytes are BLAKE2b-256
//! hashed before signing (standard Substrate signed-payload rule).

use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use subxt::utils::{AccountId32, MultiSignature};
use thiserror::Error;
use truapi::latest::{HostSignPayloadData, HostSignPayloadResponse, TxPayloadExtension};

use crate::{
	host_internal::extrinsic::build_signed_extrinsic_v4_with_signature,
	host_logic::product_account::SR25519_SIGNING_CONTEXT,
};

/// Preimages longer than this are hashed before signing.
const MAX_SIGNED_PREIMAGE_LEN: usize = 256;

/// Error assembling a Substrate extrinsic signing preimage.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExtrinsicPayloadError {
	/// Payload lists a signed extension this host cannot encode from
	/// `HostSignPayloadData`.
	#[error(
		"unsupported signed extension `{id}`: its encoded fields are not present in HostSignPayloadData"
	)]
	UnsupportedSignedExtension {
		/// Extension identifier from `signed_extensions`.
		id: String,
	},
	/// `CheckMetadataHash.mode` is SCALE-encoded as a single byte.
	#[error("CheckMetadataHash mode {mode} does not fit in a u8")]
	MetadataHashModeOutOfRange {
		/// Supplied mode value.
		mode: u32,
	},
	/// `CheckMetadataHash.metadata_hash` must be exactly 32 bytes when present.
	#[error("CheckMetadataHash metadata hash is {len} bytes, expected 32")]
	InvalidMetadataHashLength {
		/// Supplied metadata hash length.
		len: usize,
	},
	/// Only version 4 payloads carry every field this preimage needs.
	#[error("unsupported extrinsic payload version {version}; only version 4 is supported")]
	UnsupportedPayloadVersion {
		/// Version the caller asked for.
		version: u32,
	},
}

/// Sign a V4 extrinsic payload with `keypair`, assembling the signed extrinsic
/// too when the caller asked for one.
pub fn sign_extrinsic_payload(
	keypair: &schnorrkel::Keypair,
	payload: HostSignPayloadData,
) -> Result<HostSignPayloadResponse, ExtrinsicPayloadError> {
	if payload.version != 4 {
		return Err(ExtrinsicPayloadError::UnsupportedPayloadVersion { version: payload.version });
	}
	let preimage = extrinsic_payload_preimage(&payload)?;
	let raw_signature = keypair
		.secret
		.sign_simple(SR25519_SIGNING_CONTEXT, &preimage, &keypair.public)
		.to_bytes();
	let signature = MultiSignature::Sr25519(raw_signature);
	let signed_transaction = payload.with_signed_transaction.0.unwrap_or(false).then(|| {
		let extensions = extrinsic_payload_extensions(&payload)
			.expect("preimage construction already validated signed extensions");
		build_signed_extrinsic_v4_with_signature(
			AccountId32(keypair.public.to_bytes()),
			&signature,
			&payload.method,
			&extensions,
		)
	});
	Ok(HostSignPayloadResponse { signature: signature.encode(), signed_transaction })
}

/// Encode the standard signed extensions in the order declared by the target
/// runtime. Unknown extensions are rejected because this wire payload has no
/// field carrying their extra or implicit bytes.
pub fn extrinsic_payload_extensions(
	payload: &HostSignPayloadData,
) -> Result<Vec<TxPayloadExtension>, ExtrinsicPayloadError> {
	payload
		.signed_extensions
		.iter()
		.map(|id| {
			let (extra, additional_signed) = match id.as_str() {
				"CheckNonZeroSender" | "CheckWeight" => (Vec::new(), Vec::new()),
				"CheckSpecVersion" => (Vec::new(), payload.spec_version.clone()),
				"CheckTxVersion" => (Vec::new(), payload.transaction_version.clone()),
				"CheckGenesis" => (Vec::new(), payload.genesis_hash.clone()),
				"CheckMortality" => (payload.era.clone(), payload.block_hash.clone()),
				"CheckNonce" => (payload.nonce.clone(), Vec::new()),
				"ChargeTransactionPayment" => (payload.tip.clone(), Vec::new()),
				"ChargeAssetTxPayment" => {
					let mut extra = payload.tip.clone();
					match &payload.asset_id {
						Some(asset_id) => extra.extend_from_slice(asset_id),
						None => None::<()>.encode_to(&mut extra),
					}
					(extra, Vec::new())
				},
				"CheckMetadataHash" => {
					let mode = payload.mode.unwrap_or(0);
					let mode = u8::try_from(mode)
						.map_err(|_| ExtrinsicPayloadError::MetadataHashModeOutOfRange { mode })?;
					let metadata_hash = payload
						.metadata_hash
						.as_deref()
						.map(|hash| {
							<[u8; 32]>::try_from(hash).map_err(|_| {
								ExtrinsicPayloadError::InvalidMetadataHashLength { len: hash.len() }
							})
						})
						.transpose()?;
					(mode.encode(), metadata_hash.encode())
				},
				unsupported => {
					return Err(ExtrinsicPayloadError::UnsupportedSignedExtension {
						id: unsupported.to_string(),
					});
				},
			};
			Ok(TxPayloadExtension { id: id.clone(), extra, additional_signed })
		})
		.collect()
}

/// Signing preimage for an extrinsic payload:
/// `method ++ Σextension.extra ++ Σextension.additional_signed`, with both
/// extension sequences following the declared runtime order.
pub fn extrinsic_payload_preimage(
	payload: &HostSignPayloadData,
) -> Result<Vec<u8>, ExtrinsicPayloadError> {
	let extensions = extrinsic_payload_extensions(payload)?;

	let mut preimage = Vec::new();
	preimage.extend_from_slice(&payload.method);
	for extension in &extensions {
		preimage.extend_from_slice(&extension.extra);
	}
	for extension in &extensions {
		preimage.extend_from_slice(&extension.additional_signed);
	}
	Ok(hash_large_preimage(preimage))
}

fn hash_large_preimage(preimage: Vec<u8>) -> Vec<u8> {
	if preimage.len() > MAX_SIGNED_PREIMAGE_LEN {
		// This is the same primitive and threshold used by Subxt's
		// `frame_decode::extrinsics::encode_v4_signer_payload`. That builder
		// itself is not applicable here because this API receives pre-encoded
		// fields without runtime metadata or structured call arguments.
		blake2_256(&preimage).to_vec()
	} else {
		preimage
	}
}

#[cfg(test)]
mod tests;
