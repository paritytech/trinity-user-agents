//! sr25519 transaction signing shared by chain-facing runtime services.
//!
//! `Sr25519Signer` is the one subxt [`Signer`] in the crate; Bulletin
//! preimage submission and the signing-host product key both go through it.
//! `build_signed_extrinsic_v4` assembles a signed V4 extrinsic from
//! caller-supplied, already-SCALE-encoded parts (local `create_transaction`),
//! so it needs no metadata at all. `build_signed_extrinsic_v5` needs the
//! runtime's extension pipeline, and signs only when the caller leaves
//! `VerifyMultiSignature` to the host.

use core::time::Duration;
use std::collections::BTreeSet;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use parity_scale_codec::Encode;
use schnorrkel::{PublicKey, SecretKey};
use subxt::{
	config::{
		ClientState, TransactionExtension as SubxtTransactionExtension, substrate::SubstrateConfig,
		transaction_extensions::VerifySignature,
	},
	ext::{
		frame_decode::extrinsics::{
			ExtrinsicTypeInfo, TransactionExtension as FrameTransactionExtension,
			TransactionExtensions as FrameTransactionExtensions, TransactionExtensionsError,
			encode_v5_general_with_info_to, encode_v5_signer_payload_with_info,
		},
		scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		scale_encode::{EncodeAsFields, Error as ScaleEncodeError, FieldIter, TypeResolver},
	},
	metadata::ArcMetadata,
	tx::Signer,
	utils::{AccountId32, H256, MultiAddress, MultiSignature},
};
use tracing::warn;
use truapi::latest::TxPayloadExtension;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::{
	chain_runtime::{ChainRuntime, CurrentBlockError, encode_hex},
	host_logic::product_account::SR25519_SIGNING_CONTEXT,
};

/// Waiting longer than this for the current block is named in the host log.
const SLOW_CURRENT_BLOCK: Duration = Duration::from_secs(2);

/// The runtime name of the V5 authorization extension.
///
/// Takes the resolver it is resolved against, never reading it, because the one
/// caller that needs a value form cannot name its resolver: `PortableRegistry`
/// comes from `scale-info`, which is not a `wasm32` dependency and is not
/// re-exported through `subxt::ext`. Contexts that are themselves generic over
/// `R` spell the associated const directly instead.
fn verify_multi_signature_name<R>(_types: &R) -> &'static str
where
	R: TypeResolver,
	VerifySignature<SubstrateConfig>: FrameTransactionExtension<R>,
{
	<VerifySignature<SubstrateConfig> as FrameTransactionExtension<R>>::NAME
}

/// Parse a 64-byte sr25519 secret in either of the two wire encodings.
///
/// Rust-generated keys use schnorrkel's canonical scalar bytes; legacy
/// JS-derived keys use scure/ed25519-expanded scalar bytes. Signatures are
/// identical for both once parsed, so callers never need to know which form
/// they hold.
pub fn sr25519_secret_from_bytes(secret: &[u8; 64]) -> Result<SecretKey, String> {
	match SecretKey::from_bytes(secret) {
		Ok(secret) => Ok(secret),
		Err(canonical_error) => SecretKey::from_ed25519_bytes(secret).map_err(|ed_error| {
			format!("invalid sr25519 secret: canonical={canonical_error}; ed25519={ed_error}")
		}),
	}
}

/// sr25519 [`Signer`] over a parsed schnorrkel key.
///
/// Holds only the parsed key (schnorrkel zeroizes it on drop), never the raw
/// secret bytes.
#[derive(derive_more::Debug)]
pub struct Sr25519Signer {
	#[debug("\"<redacted>\"")]
	secret: SecretKey,
	#[debug("{}", crate::logging::Account::new(&public.to_bytes()))]
	public: PublicKey,
}

impl Sr25519Signer {
	/// Parse a signer from 64-byte secret material.
	pub fn from_secret_bytes(secret: &[u8; 64]) -> Result<Self, String> {
		let secret = sr25519_secret_from_bytes(secret)?;
		let public = secret.to_public();
		Ok(Self { secret, public })
	}

	/// Build a signer from an already-derived schnorrkel keypair (e.g. a
	/// signing-host product key), reusing the same `sign`/`account_id` path.
	pub fn from_keypair(keypair: &schnorrkel::Keypair) -> Self {
		Self { secret: keypair.secret.clone(), public: keypair.public }
	}
}

impl Signer<SubstrateConfig> for Sr25519Signer {
	fn account_id(&self) -> AccountId32 {
		AccountId32(self.public.to_bytes())
	}

	fn sign(&self, signer_payload: &[u8]) -> MultiSignature {
		let signature =
			self.secret.sign_simple(SR25519_SIGNING_CONTEXT, signer_payload, &self.public);
		MultiSignature::Sr25519(signature.to_bytes())
	}
}

/// The V4 signer payload: `call_data ++ Σextra ++ Σadditional_signed`, replaced
/// by its blake2_256 hash only when it exceeds 256 bytes.
///
/// Note the order differs from the extrinsic body (which puts `extra` before
/// the call): the call comes first here, extras next, implicits last.
fn v4_signer_payload(call_data: &[u8], extensions: &[TxPayloadExtension]) -> Vec<u8> {
	let mut payload = Vec::with_capacity(call_data.len());
	payload.extend_from_slice(call_data);
	for ext in extensions {
		payload.extend_from_slice(&ext.extra);
	}
	for ext in extensions {
		payload.extend_from_slice(&ext.additional_signed);
	}
	if payload.len() > 256 { sp_crypto_hashing::blake2_256(&payload).to_vec() } else { payload }
}

/// Assemble a signed Extrinsic V4 from caller-supplied, already-SCALE-encoded
/// parts, entirely offline.
///
/// Body layout (matches `frame_decode::encode_v4_signed` and subxt's own
/// assembler byte-for-byte):
///
/// ```text
/// Compact(len) ++ 0x84 ++ 0x00 ++ signer(32) ++ 0x01 ++ signature(64)
///              ++ Σ extension.extra ++ call_data
/// ```
///
/// `0x84` is the V4 "signed" version byte, `0x00` the `MultiAddress::Id`
/// discriminant, `0x01` the `MultiSignature::Sr25519` discriminant. `extra`
/// bytes go in the body (in the given order, which must be the runtime's
/// canonical extension order); `additional_signed` bytes appear only in the
/// signed payload. The chain binding (genesis, spec/tx version, mortality
/// anchor, nonce, tip) lives inside the caller's extension bytes, so nothing
/// here is metadata-driven.
fn build_signed_extrinsic_v4(
	signer: &Sr25519Signer,
	call_data: &[u8],
	extensions: &[TxPayloadExtension],
) -> Vec<u8> {
	let signature = signer.sign(&v4_signer_payload(call_data, extensions));
	build_signed_extrinsic_v4_with_signature(signer.account_id(), &signature, call_data, extensions)
}

/// Assemble a signed Extrinsic V4 using an existing signature.
///
/// This keeps `signPayload`'s returned typed signature byte-for-byte identical
/// to the signature embedded in an optionally requested signed transaction.
pub fn build_signed_extrinsic_v4_with_signature(
	account_id: AccountId32,
	signature: &MultiSignature,
	call_data: &[u8],
	extensions: &[TxPayloadExtension],
) -> Vec<u8> {
	/// `0b1000_0000 | 4`: the "signed" bit plus extrinsic format version 4.
	const EXTRINSIC_V4_SIGNED: u8 = 0x84;

	let address = MultiAddress::<AccountId32, u32>::Id(account_id);
	let mut inner = Vec::new();
	inner.push(EXTRINSIC_V4_SIGNED);
	address.encode_to(&mut inner);
	signature.encode_to(&mut inner);
	for ext in extensions {
		inner.extend_from_slice(&ext.extra);
	}
	inner.extend_from_slice(call_data);

	// `Vec<u8>::encode` prepends the SCALE compact length, giving the outer
	// length-prefixed opaque extrinsic.
	inner.encode()
}

/// Call arguments that are already SCALE encoded by the product.
///
/// `frame-decode` still resolves the pallet and call from metadata, but the
/// request contract deliberately carries opaque argument bytes rather than a
/// typed value tree. The first two bytes (pallet and call indices) are handled
/// separately; this adapter writes the remaining bytes unchanged.
struct PreencodedCallArgs<'a>(&'a [u8]);

impl EncodeAsFields for PreencodedCallArgs<'_> {
	fn encode_as_fields_to<R: TypeResolver>(
		&self,
		_fields: &mut dyn FieldIter<'_, R::TypeId>,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), ScaleEncodeError> {
		out.extend_from_slice(self.0);
		Ok(())
	}
}

/// Caller-supplied opaque transaction-extension bytes, with V5 signature
/// handling delegated to Subxt's `VerifySignature` unless the caller supplied
/// that extension itself.
struct PreencodedExtensions<'a> {
	supplied: &'a [TxPayloadExtension],
	/// `None` when the caller supplied `VerifyMultiSignature` bytes.
	verify_signature: Option<VerifySignature<SubstrateConfig>>,
}

impl PreencodedExtensions<'_> {
	fn supplied(&self, name: &str) -> Result<&TxPayloadExtension, TransactionExtensionsError> {
		self.supplied.iter().find(|extension| extension.id == name).ok_or_else(|| {
			TransactionExtensionsError::Other {
				extension_name: name.to_string(),
				error: "the caller supplied no bytes for this transaction extension".into(),
			}
		})
	}

	/// The host-owned signature extension, when `name` names it.
	fn host_verify_signature<R>(&self, name: &str) -> Option<&VerifySignature<SubstrateConfig>>
	where
		R: TypeResolver,
		VerifySignature<SubstrateConfig>: FrameTransactionExtension<R>,
	{
		self.verify_signature.as_ref().filter(|_| {
			name == <VerifySignature<SubstrateConfig> as FrameTransactionExtension<R>>::NAME
		})
	}
}

impl<R> FrameTransactionExtensions<R> for PreencodedExtensions<'_>
where
	R: TypeResolver,
	VerifySignature<SubstrateConfig>: FrameTransactionExtension<R>,
{
	fn contains_extension(&self, name: &str) -> bool {
		self.host_verify_signature::<R>(name).is_some() ||
			self.supplied.iter().any(|extension| extension.id == name)
	}

	fn is_authorization_extension(&self, name: &str) -> bool {
		// Only `VerifyMultiSignature` may answer true: the signer payload is
		// cut at the last extension that does, and this one's position defines
		// that cut. Another name here — `AsResources`, whose implication covers
		// a later span — moves the cut and signs the wrong scope.
		name == <VerifySignature<SubstrateConfig> as FrameTransactionExtension<R>>::NAME
	}

	fn encode_extension_value_to(
		&self,
		name: &str,
		type_id: R::TypeId,
		type_resolver: &R,
		out: &mut Vec<u8>,
	) -> Result<(), TransactionExtensionsError> {
		if let Some(verify_signature) = self.host_verify_signature::<R>(name) {
			return verify_signature.encode_value_to(type_id, type_resolver, out).map_err(
				|error| TransactionExtensionsError::Other {
					extension_name: name.to_string(),
					error,
				},
			);
		}
		out.extend_from_slice(&self.supplied(name)?.extra);
		Ok(())
	}

	fn encode_extension_implicit_to(
		&self,
		name: &str,
		type_id: R::TypeId,
		type_resolver: &R,
		out: &mut Vec<u8>,
	) -> Result<(), TransactionExtensionsError> {
		if let Some(verify_signature) = self.host_verify_signature::<R>(name) {
			return verify_signature.encode_implicit_to(type_id, type_resolver, out).map_err(
				|error| TransactionExtensionsError::Other {
					extension_name: name.to_string(),
					error,
				},
			);
		}
		out.extend_from_slice(&self.supplied(name)?.additional_signed);
		Ok(())
	}
}

/// Why a locally assembled transaction could not be produced.
#[derive(Debug, derive_more::Display)]
pub enum LocalTransactionError {
	/// A transaction extension version the runtime does not declare.
	#[display(
		"unsupported tx_ext_version {version}; the runtime declares transaction extension \
         versions {declared:?}"
	)]
	UnsupportedTxExtVersion {
		/// Version the caller asked for.
		version: u8,
		/// Versions the runtime metadata declares.
		declared: Vec<u8>,
	},
	/// The host could not connect to the chain, for instance because it serves
	/// no such chain.
	#[display("{_0}")]
	NoChainConnection(String),
	/// The node could not be asked for the current block or its runtime, such
	/// as during a halt, or did not answer in time. A later call can succeed.
	#[display("{_0}")]
	ChainUnavailable(String),
	/// The caller's extension list does not fit the runtime's pipeline.
	#[display("{_0}")]
	UnsupportedExtensions(String),
	/// Any other assembly failure.
	#[display("{_0}")]
	Other(String),
}

/// Runtime metadata for locally constructing a transaction.
pub async fn local_transaction_metadata(
	chain: &ChainRuntime,
	genesis_hash: [u8; 32],
) -> Result<ArcMetadata, LocalTransactionError> {
	let started = Instant::now();
	let at_block = chain.current_block(&genesis_hash).await.map_err(|error| match error {
		CurrentBlockError::NoChainConnection(failure) => LocalTransactionError::NoChainConnection(
			format!("cannot load chain metadata: {failure}"),
		),
		CurrentBlockError::ChainUnavailable(reason) => LocalTransactionError::ChainUnavailable(
			format!("cannot select a metadata block: {reason}"),
		),
		CurrentBlockError::Other(reason) => {
			LocalTransactionError::Other(format!("cannot select a metadata block: {reason}"))
		},
	});
	let waited = started.elapsed();
	if waited > SLOW_CURRENT_BLOCK || at_block.is_err() {
		let genesis = encode_hex(&genesis_hash);
		match &at_block {
			Err(error) => warn!(
				"createTransaction could not get the current block of {genesis} after {waited:?}: {error}"
			),
			Ok(_) => {
				warn!("createTransaction waited {waited:?} for the current block of {genesis}")
			},
		}
	}
	Ok(at_block?.metadata())
}

/// Choose the extrinsic format for `tx_ext_version`, then assemble it.
///
/// V4 always uses transaction extension version 0, so a non-zero version
/// builds V5. Version 0 builds V5 while it includes `VerifyMultiSignature`,
/// since only then can a general transaction carry a signature, and V4
/// otherwise.
pub fn build_signed_transaction(
	signer: &Sr25519Signer,
	genesis_hash: [u8; 32],
	call_data: &[u8],
	extensions: &[TxPayloadExtension],
	tx_ext_version: u8,
	metadata: ArcMetadata,
) -> Result<Vec<u8>, LocalTransactionError> {
	let declared = metadata
		.extrinsic_extension_version_info()
		.map_err(|error| {
			LocalTransactionError::Other(format!("transaction extension versions: {error}"))
		})?
		.collect::<Vec<_>>();
	if !declared.contains(&tx_ext_version) {
		return Err(LocalTransactionError::UnsupportedTxExtVersion {
			version: tx_ext_version,
			declared,
		});
	}
	if tx_ext_version == 0 && !declares_verify_multi_signature(&metadata, 0)? {
		return Ok(build_signed_extrinsic_v4(signer, call_data, extensions));
	}
	build_signed_extrinsic_v5(signer, genesis_hash, call_data, extensions, tx_ext_version, metadata)
}

/// Whether the given transaction extension version includes `VerifyMultiSignature`.
fn declares_verify_multi_signature(
	metadata: &ArcMetadata,
	transaction_extension_version: u8,
) -> Result<bool, LocalTransactionError> {
	let verify_multi_signature = verify_multi_signature_name(metadata.types());
	Ok(metadata
		.extrinsic_extension_info(Some(transaction_extension_version))
		.map_err(|error| {
			LocalTransactionError::Other(format!("transaction extension metadata: {error}"))
		})?
		.extension_ids
		.iter()
		.any(|declared| declared.name == verify_multi_signature))
}

/// Whether a type encodes to zero bytes, mirroring the private `is_type_empty`
/// filter `frame-decode` 0.18 applies before asking for implicit bytes. Probed by
/// traversing empty input, which can only ever over-validate: every kind that
/// crate treats as non-empty needs at least one byte, so it cannot decode from
/// none.
fn type_is_empty<R: TypeResolver>(type_id: R::TypeId, types: &R) -> bool {
	decode_with_visitor(&mut &[][..], type_id, types, IgnoreVisitor::new()).is_ok()
}

/// Check that `bytes` traverse `type_id` and leave nothing over.
///
/// `decode_with_visitor` applies no depth limit, and here the recursion depth
/// is set by product-supplied bytes against a type graph that lives in chain
/// metadata rather than in any Rust definition. That is safe only while the
/// extension types stay flat, which they are today; a recursive extension type
/// would need the bound that guards decoding elsewhere.
fn traverse_exactly<R: TypeResolver>(
	bytes: &[u8],
	type_id: R::TypeId,
	types: &R,
	what: &str,
) -> Result<(), LocalTransactionError> {
	let mut cursor = bytes;
	decode_with_visitor(&mut cursor, type_id, types, IgnoreVisitor::new()).map_err(|error| {
		LocalTransactionError::UnsupportedExtensions(format!(
			"supplied {what} ({} byte(s)) does not decode: {error}",
			bytes.len()
		))
	})?;
	if !cursor.is_empty() {
		return Err(LocalTransactionError::UnsupportedExtensions(format!(
			"supplied {what} has {} trailing byte(s)",
			cursor.len()
		)));
	}
	Ok(())
}

/// Assemble and sign an Extrinsic V5 General transaction from the product's
/// pre-encoded call and extension fields.
///
/// Metadata selects the transaction-extension pipeline version and supplies
/// its ordering/type information. `frame-decode` owns the V5 implication hash
/// and final wire encoding; Subxt owns `VerifyMultiSignature`'s Disabled/Signed
/// behavior and signature representation.
///
/// `VerifyMultiSignature` is host-owned only while `extensions` omits it. A
/// caller that supplies it owns its bytes and gets no host signature, whether
/// they say `Disabled` (authorizing with a proof in a later extension) or carry
/// a signature the caller made itself.
fn build_signed_extrinsic_v5(
	signer: &Sr25519Signer,
	genesis_hash: [u8; 32],
	call_data: &[u8],
	extensions: &[TxPayloadExtension],
	transaction_extension_version: u8,
	metadata: ArcMetadata,
) -> Result<Vec<u8>, LocalTransactionError> {
	let other = LocalTransactionError::Other;
	let (&pallet_index, rest) = call_data
		.split_first()
		.ok_or_else(|| other("V5 call data is missing its pallet index".to_string()))?;
	let (&call_index, call_args) = rest
		.split_first()
		.ok_or_else(|| other("V5 call data is missing its call index".to_string()))?;
	let call_info = metadata
		.extrinsic_call_info_by_index(pallet_index, call_index)
		.map_err(|error| other(format!("V5 call metadata: {error}")))?;
	let extension_info = metadata
		.extrinsic_extension_info(Some(transaction_extension_version))
		.map_err(|error| other(format!("V5 transaction-extension metadata: {error}")))?;

	// `VerifySignature::new` does not inspect client state, but constructing it
	// through Subxt's public trait keeps ownership of its V5 semantics there.
	let client_state = ClientState {
		genesis_hash: H256(genesis_hash),
		spec_version: 0,
		transaction_version: 0,
		metadata: metadata.clone(),
	};

	// Matched against every version the runtime declares: a product reading the
	// legacy flat `Metadata_metadata` sees pipeline version zero, while encoding
	// uses the version the runtime nominates, and those sets can differ. An id
	// in no version is a typo or a stale build; one declared elsewhere is
	// surplus, and the encoder never asks for its bytes.
	let mut declared_at_any_version = BTreeSet::new();
	for version in metadata
		.extrinsic_extension_version_info()
		.map_err(|error| other(format!("V5 transaction-extension versions: {error}")))?
	{
		for declared in metadata
			.extrinsic_extension_info(Some(version))
			.map_err(|error| other(format!("V5 transaction-extension metadata: {error}")))?
			.extension_ids
		{
			declared_at_any_version.insert(declared.name.into_owned());
		}
	}
	let mut seen_ids = BTreeSet::new();
	for extension in extensions {
		if !seen_ids.insert(extension.id.as_str()) {
			return Err(LocalTransactionError::UnsupportedExtensions(format!(
				"transaction extension {:?} is listed more than once; every \
                 reader takes the first entry, so the rest would be dropped in \
                 silence",
				extension.id
			)));
		}
		if !declared_at_any_version.contains(extension.id.as_str()) {
			return Err(LocalTransactionError::UnsupportedExtensions(format!(
				"transaction extension {:?} is not declared by the runtime at \
                 any pipeline version; encoding uses version \
                 {transaction_extension_version}, and the declared names across \
                 all versions are [{}]",
				extension.id,
				declared_at_any_version.iter().cloned().collect::<Vec<_>>().join(", ")
			)));
		}
	}

	let verify_multi_signature = verify_multi_signature_name(metadata.types());
	let supplied_verify_signature =
		extensions.iter().find(|extension| extension.id == verify_multi_signature);
	let declared_verify_signature = extension_info
		.extension_ids
		.iter()
		.find(|declared| declared.name == verify_multi_signature);
	match (supplied_verify_signature, declared_verify_signature) {
		// The caller owns the authorization slot, so its bytes must fit.
		(Some(supplied), Some(declared)) => traverse_exactly(
			&supplied.extra,
			declared.id,
			metadata.types(),
			verify_multi_signature,
		)?,
		// Declining to sign is the caller's whole intent, but the version being
		// encoded has no slot to put their bytes in. Silently dropping the one
		// extension that authorizes the transaction is not a safe default.
		(Some(_), None) => {
			return Err(LocalTransactionError::UnsupportedExtensions(format!(
				"pipeline version {transaction_extension_version} does not declare \
                 {verify_multi_signature}, so the supplied value cannot authorize \
                 this transaction"
			)));
		},
		(None, Some(_)) => {},
		// Nothing supplies the slot and the encoding version has none, so the
		// host's signature has nowhere to go. Returning the transaction anyway
		// hands back one that carries no authorization at all.
		(None, None) => {
			return Err(LocalTransactionError::UnsupportedExtensions(format!(
				"pipeline version {transaction_extension_version} does not declare \
                 {verify_multi_signature}, so the host cannot authorize this \
                 transaction"
			)));
		},
	}

	let verify_signature = supplied_verify_signature
		.is_none()
		.then(|| {
			<VerifySignature<SubstrateConfig> as SubxtTransactionExtension<SubstrateConfig>>::new(
				&client_state,
				(),
			)
			.map_err(|error| other(format!("V5 signature extension: {error}")))
		})
		.transpose()?;
	let mut transaction_extensions =
		PreencodedExtensions { supplied: extensions, verify_signature };
	let call_args = PreencodedCallArgs(call_args);

	if transaction_extensions.verify_signature.is_some() {
		// The signer payload hashes both blobs of every extension after the
		// authorization cut, taken verbatim from the caller: the values that
		// also land in the body, then the implicits that appear nowhere else. A
		// short or malformed one on either side signs a different payload than
		// the node recomputes, which surfaces as a BadProof with nothing local
		// to read.
		let signed_from = extension_info
			.extension_ids
			.iter()
			.rposition(|declared| declared.name == verify_multi_signature)
			.map_or(0, |last| last + 1);
		for declared in &extension_info.extension_ids[signed_from..] {
			let Some(supplied) = extensions.iter().find(|extension| extension.id == declared.name)
			else {
				continue;
			};
			if !type_is_empty(declared.id, metadata.types()) {
				traverse_exactly(
					&supplied.extra,
					declared.id,
					metadata.types(),
					&format!("{} value", declared.name),
				)?;
			}
			if !type_is_empty(declared.implicit_id, metadata.types()) {
				traverse_exactly(
					&supplied.additional_signed,
					declared.implicit_id,
					metadata.types(),
					&format!("{} implicit", declared.name),
				)?;
			}
		}

		let signer_payload = encode_v5_signer_payload_with_info(
			&call_args,
			transaction_extension_version,
			&transaction_extensions,
			metadata.types(),
			&call_info,
			&extension_info,
		)
		.map_err(|error| other(format!("V5 signer payload: {error}")))?;
		let signature = signer.sign(&signer_payload);
		if let Some(verify_signature) = transaction_extensions.verify_signature.as_mut() {
			verify_signature.inject_signature(&signer.account_id(), &signature);
		}
	}

	let mut transaction = Vec::new();
	encode_v5_general_with_info_to(
		&call_args,
		transaction_extension_version,
		&transaction_extensions,
		metadata.types(),
		&call_info,
		&extension_info,
		&mut transaction,
	)
	.map_err(|error| LocalTransactionError::Other(format!("V5 transaction: {error}")))?;
	Ok(transaction)
}

/// Signer and transaction-assembly tests, plus offline-chain fixtures reused by other
/// test modules.
#[cfg(test)]
pub mod tests;
