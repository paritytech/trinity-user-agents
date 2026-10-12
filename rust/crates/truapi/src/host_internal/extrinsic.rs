//! sr25519 transaction signing shared by chain-facing runtime services.
//!
//! `Sr25519Signer` is the one subxt [`Signer`] in the crate; Bulletin
//! preimage submission and the signing-host product key both go through it.
//! `build_signed_extrinsic_v4` assembles a signed V4 extrinsic from
//! caller-supplied, already-SCALE-encoded parts (local `create_transaction`),
//! so it needs no metadata at all. `build_signed_extrinsic_v5` needs the
//! runtime's extension pipeline, and signs only when the caller leaves
//! `VerifyMultiSignature` to the host.

use std::collections::BTreeSet;

use parity_scale_codec::Encode;
use schnorrkel::{PublicKey, SecretKey};
use subxt::config::substrate::SubstrateConfig;
use subxt::config::transaction_extensions::VerifySignature;
use subxt::config::{ClientState, TransactionExtension as SubxtTransactionExtension};
use subxt::ext::frame_decode::extrinsics::{
    ExtrinsicTypeInfo, TransactionExtension as FrameTransactionExtension,
    TransactionExtensions as FrameTransactionExtensions, TransactionExtensionsError,
    encode_v5_general_with_info_to, encode_v5_signer_payload_with_info,
};
use subxt::ext::scale_decode::visitor::{IgnoreVisitor, decode_with_visitor};
use subxt::ext::scale_encode::{
    EncodeAsFields, Error as ScaleEncodeError, FieldIter, TypeResolver,
};
use subxt::metadata::ArcMetadata;
use subxt::tx::Signer;
use subxt::utils::{AccountId32, H256, MultiAddress, MultiSignature};
use truapi::latest::{HostCreateTransactionResponse, TxPayloadExtension};

use crate::chain_runtime::ChainRuntime;
use crate::host_logic::product_account::SR25519_SIGNING_CONTEXT;

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
    #[debug("{}", hex::encode(public.to_bytes()))]
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
        Self {
            secret: keypair.secret.clone(),
            public: keypair.public,
        }
    }
}

impl Signer<SubstrateConfig> for Sr25519Signer {
    fn account_id(&self) -> AccountId32 {
        AccountId32(self.public.to_bytes())
    }

    fn sign(&self, signer_payload: &[u8]) -> MultiSignature {
        let signature =
            self.secret
                .sign_simple(SR25519_SIGNING_CONTEXT, signer_payload, &self.public);
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
    if payload.len() > 256 {
        sp_crypto_hashing::blake2_256(&payload).to_vec()
    } else {
        payload
    }
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
pub fn build_signed_extrinsic_v4(
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
        self.supplied
            .iter()
            .find(|extension| extension.id == name)
            .ok_or_else(|| TransactionExtensionsError::Other {
                extension_name: name.to_string(),
                error: "the caller supplied no bytes for this transaction extension".into(),
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
        self.host_verify_signature::<R>(name).is_some()
            || self.supplied.iter().any(|extension| extension.id == name)
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
            return verify_signature
                .encode_value_to(type_id, type_resolver, out)
                .map_err(|error| TransactionExtensionsError::Other {
                    extension_name: name.to_string(),
                    error,
                });
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
            return verify_signature
                .encode_implicit_to(type_id, type_resolver, out)
                .map_err(|error| TransactionExtensionsError::Other {
                    extension_name: name.to_string(),
                    error,
                });
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
    /// The runtime metadata needed to choose and encode the format is unavailable.
    #[display("{_0}")]
    ChainUnavailable(String),
    /// The caller's extension list does not fit the runtime's pipeline.
    #[display("{_0}")]
    UnsupportedExtensions(String),
    /// Any other assembly failure.
    #[display("{_0}")]
    Other(String),
}

/// Assemble a transaction locally from caller-supplied, pre-encoded parts,
/// against the runtime metadata of the genesis-pinned Subxt client.
pub async fn build_local_transaction(
    chain: &ChainRuntime,
    keypair: &schnorrkel::Keypair,
    genesis_hash: [u8; 32],
    call_data: &[u8],
    extensions: &[TxPayloadExtension],
    tx_ext_version: u8,
) -> Result<HostCreateTransactionResponse, LocalTransactionError> {
    let client = chain.online_client(&genesis_hash).await.map_err(|error| {
        LocalTransactionError::ChainUnavailable(format!("cannot load chain metadata: {error}"))
    })?;
    let at_block = client.at_current_block().await.map_err(|error| {
        LocalTransactionError::ChainUnavailable(format!("cannot select a metadata block: {error}"))
    })?;
    let transaction = build_signed_transaction(
        &Sr25519Signer::from_keypair(keypair),
        genesis_hash,
        call_data,
        extensions,
        tx_ext_version,
        at_block.metadata(),
    )?;
    Ok(HostCreateTransactionResponse { transaction })
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
    build_signed_extrinsic_v5(
        signer,
        genesis_hash,
        call_data,
        extensions,
        tx_ext_version,
        metadata,
    )
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
pub fn build_signed_extrinsic_v5(
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
                declared_at_any_version
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    let verify_multi_signature = verify_multi_signature_name(metadata.types());
    let supplied_verify_signature = extensions
        .iter()
        .find(|extension| extension.id == verify_multi_signature);
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
        }
        (None, Some(_)) => {}
        // Nothing supplies the slot and the encoding version has none, so the
        // host's signature has nowhere to go. Returning the transaction anyway
        // hands back one that carries no authorization at all.
        (None, None) => {
            return Err(LocalTransactionError::UnsupportedExtensions(format!(
                "pipeline version {transaction_extension_version} does not declare \
                 {verify_multi_signature}, so the host cannot authorize this \
                 transaction"
            )));
        }
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
    let mut transaction_extensions = PreencodedExtensions {
        supplied: extensions,
        verify_signature,
    };
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
            let Some(supplied) = extensions
                .iter()
                .find(|extension| extension.id == declared.name)
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
pub mod tests {
    use super::*;
    use crate::runtime::statement_allowance::collection::PersonhoodCollection;
    use parity_scale_codec::{Compact, Decode, Encode};
    use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive};
    use subxt::client::{OfflineClient, OfflineClientAtBlock};
    use subxt::config::substrate::{SpecVersionForRange, SubstrateConfigBuilder};
    use subxt::events::Phase;
    use subxt::ext::frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};
    use subxt::ext::scale_encode::{EncodeAsFields, Field};
    use subxt::ext::scale_value::{Primitive, Value as ScaleValue};
    use subxt::metadata::{ArcMetadata, Metadata};
    use subxt::utils::H256;

    use crate::runtime::statement_allowance::extension::{
        ChainState, Metadata as AllowanceMetadata,
    };

    /// Everything needed to build transactions offline for one chain at one
    /// runtime version, mirroring what the production path gets from the
    /// genesis-pinned online client.
    pub struct OfflineChainState {
        pub genesis_hash: [u8; 32],
        pub spec_version: u32,
        pub transaction_version: u32,
        pub metadata: ArcMetadata,
    }

    impl OfflineChainState {
        /// Build an offline subxt client pinned at `block_number`.
        pub fn client_at(
            &self,
            block_number: u64,
        ) -> Result<OfflineClientAtBlock<SubstrateConfig>, String> {
            let config = SubstrateConfigBuilder::new()
                .set_genesis_hash(H256(self.genesis_hash))
                .set_spec_version_for_block_ranges([SpecVersionForRange {
                    block_range: 0..u64::MAX,
                    spec_version: self.spec_version,
                    transaction_version: self.transaction_version,
                }])
                .set_metadata_for_spec_versions([(self.spec_version, self.metadata.clone())])
                .build();
            OfflineClient::new_with_config(config)
                .at_block(block_number)
                .map_err(|err| format!("offline client unavailable: {err}"))
        }
    }

    /// Raw `RuntimeMetadataPrefixed` bytes captured from the live
    /// bulletin-paseo chain (`state_getMetadata`, spec 1000020, metadata v14).
    pub const BULLETIN_METADATA_BYTES: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bulletin_paseo_metadata.scale"
    ));

    /// Decode the checked-in bulletin metadata fixture.
    pub fn bulletin_metadata() -> Metadata {
        let prefixed = RuntimeMetadataPrefixed::decode(&mut &BULLETIN_METADATA_BYTES[..]).unwrap();
        Metadata::try_from(prefixed).unwrap()
    }

    /// Offline chain state over the bulletin fixture with a recognizable
    /// genesis hash.
    pub fn bulletin_chain_state() -> OfflineChainState {
        OfflineChainState {
            genesis_hash: [0xbb; 32],
            spec_version: 1_000_020,
            transaction_version: 1,
            metadata: ArcMetadata::from(bulletin_metadata()),
        }
    }

    /// Output of the runtime calls a subxt client makes to load the bulletin
    /// fixture: `Core_version` (spec 1) and the metadata calls.
    pub fn bulletin_runtime_call(method: &str) -> Option<Vec<u8>> {
        Some(match method {
            "Core_version" => (
                "bulletin",
                "bulletin",
                1u32,
                1u32,
                1u32,
                Vec::<([u8; 8], u32)>::new(),
                1u32,
            )
                .encode(),
            "Metadata_metadata_versions" => vec![14u32].encode(),
            "Metadata_metadata_at_version" => {
                let mut output = vec![1];
                Compact(u32::try_from(BULLETIN_METADATA_BYTES.len()).unwrap())
                    .encode_to(&mut output);
                output.extend_from_slice(BULLETIN_METADATA_BYTES);
                output
            }
            _ => return None,
        })
    }

    /// Encoded `System.Events` over the bulletin fixture: one event per
    /// `(extrinsic index, System event name)`, each with default field values.
    pub fn system_events(events: &[(u32, &str)]) -> Vec<u8> {
        let metadata = ArcMetadata::from(bulletin_metadata());
        let system = metadata.pallet_by_name("System").unwrap();
        let mut bytes = Vec::new();
        Compact(u32::try_from(events.len()).unwrap()).encode_to(&mut bytes);
        for (extrinsic_index, event_name) in events {
            let event = system
                .event_variants()
                .unwrap()
                .iter()
                .find(|event| event.name == *event_name)
                .unwrap();
            let values = ScaleValue::unnamed_composite(
                event
                    .fields
                    .iter()
                    .map(|field| default_value(metadata.types(), field.ty.id)),
            );
            let mut fields = event
                .fields
                .iter()
                .map(|field| Field::new(field.ty.id, field.name.as_deref()));
            Phase::ApplyExtrinsic(*extrinsic_index).encode_to(&mut bytes);
            system.event_index().encode_to(&mut bytes);
            event.index.encode_to(&mut bytes);
            values
                .encode_as_fields_to(&mut fields, metadata.types(), &mut bytes)
                .unwrap();
            Vec::<[u8; 32]>::new().encode_to(&mut bytes);
        }
        bytes
    }

    /// The first-variant, zero-valued instance of a metadata type.
    fn default_value(types: &PortableRegistry, type_id: u32) -> ScaleValue {
        let ty = types.resolve(type_id).expect("metadata type exists");
        match &ty.type_def {
            TypeDef::Composite(composite) => ScaleValue::unnamed_composite(
                composite
                    .fields
                    .iter()
                    .map(|field| default_value(types, field.ty.id)),
            ),
            TypeDef::Variant(variants) => {
                let variant = variants.variants.first().expect("variant exists");
                ScaleValue::unnamed_variant(
                    variant.name.clone(),
                    variant
                        .fields
                        .iter()
                        .map(|field| default_value(types, field.ty.id)),
                )
            }
            TypeDef::Sequence(_) => ScaleValue::unnamed_composite([]),
            TypeDef::Array(array) => ScaleValue::unnamed_composite(
                (0..array.len).map(|_| default_value(types, array.type_param.id)),
            ),
            TypeDef::Tuple(tuple) => ScaleValue::unnamed_composite(
                tuple
                    .fields
                    .iter()
                    .map(|field| default_value(types, field.id)),
            ),
            TypeDef::Primitive(primitive) => match primitive {
                TypeDefPrimitive::Bool => ScaleValue::bool(false),
                TypeDefPrimitive::Char => ScaleValue::char('\0'),
                TypeDefPrimitive::Str => ScaleValue::string(""),
                TypeDefPrimitive::U8
                | TypeDefPrimitive::U16
                | TypeDefPrimitive::U32
                | TypeDefPrimitive::U64
                | TypeDefPrimitive::U128 => ScaleValue::u128(0),
                TypeDefPrimitive::U256 => ScaleValue::primitive(Primitive::U256([0; 32])),
                TypeDefPrimitive::I8
                | TypeDefPrimitive::I16
                | TypeDefPrimitive::I32
                | TypeDefPrimitive::I64
                | TypeDefPrimitive::I128 => ScaleValue::i128(0),
                TypeDefPrimitive::I256 => ScaleValue::primitive(Primitive::I256([0; 32])),
            },
            TypeDef::Compact(_) => ScaleValue::u128(0),
            TypeDef::BitSequence(_) => {
                ScaleValue::bit_sequence(subxt::ext::scale_bits::Bits::new())
            }
        }
    }

    #[test]
    fn parses_canonical_and_ed25519_expanded_secrets() {
        let canonical: [u8; 64] = hex::decode(
            "0eef5183411d40c32446bb1cbaabd70004a17af6012a577c735d054f04059208\
             573dfc9b6ffeb1c786a16349e70f9836876a743c31c0a7a2a70727a852eec372",
        )
        .unwrap()
        .try_into()
        .unwrap();
        assert!(sr25519_secret_from_bytes(&canonical).is_ok());

        let mini = schnorrkel::MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let expanded = mini
            .expand(schnorrkel::ExpansionMode::Ed25519)
            .to_ed25519_bytes();
        let expanded: [u8; 64] = expanded.as_slice().try_into().unwrap();
        let parsed = sr25519_secret_from_bytes(&expanded).unwrap();
        assert_eq!(
            parsed.to_public().to_bytes(),
            mini.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
                .public
                .to_bytes()
        );
    }

    #[test]
    fn signer_signs_under_substrate_context() {
        let mini = schnorrkel::MiniSecretKey::from_bytes(&[9; 32]).unwrap();
        let secret: [u8; 64] = mini
            .expand(schnorrkel::ExpansionMode::Ed25519)
            .to_bytes()
            .as_slice()
            .try_into()
            .unwrap();
        let signer = Sr25519Signer::from_secret_bytes(&secret).unwrap();

        let payload = b"payload";
        let MultiSignature::Sr25519(signature) = signer.sign(payload) else {
            panic!("expected sr25519 signature");
        };
        let public = PublicKey::from_bytes(&signer.account_id().0).unwrap();
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    payload,
                    &schnorrkel::Signature::from_bytes(&signature).unwrap()
                )
                .is_ok()
        );
    }

    fn test_signer() -> Sr25519Signer {
        let keypair = schnorrkel::MiniSecretKey::from_bytes(&[3; 32])
            .unwrap()
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        Sr25519Signer::from_keypair(&keypair)
    }

    fn ext(id: &str, extra: &[u8], additional: &[u8]) -> TxPayloadExtension {
        TxPayloadExtension {
            id: id.to_string(),
            extra: extra.to_vec(),
            additional_signed: additional.to_vec(),
        }
    }

    /// Split a length-prefixed V4 signed extrinsic into
    /// (account, signature, trailing-bytes-after-signature).
    pub fn split_v4(extrinsic: &[u8]) -> ([u8; 32], [u8; 64], Vec<u8>) {
        let mut input = extrinsic;
        let len = Compact::<u32>::decode(&mut input).unwrap().0 as usize;
        assert_eq!(input.len(), len, "compact length must cover the remainder");
        assert_eq!(input[0], 0x84, "V4 signed version byte");
        assert_eq!(input[1], 0x00, "MultiAddress::Id discriminant");
        let account: [u8; 32] = input[2..34].try_into().unwrap();
        assert_eq!(input[34], 0x01, "MultiSignature::Sr25519 discriminant");
        let signature: [u8; 64] = input[35..99].try_into().unwrap();
        (account, signature, input[99..].to_vec())
    }

    #[test]
    fn v4_layout_and_signature_verify() {
        let signer = test_signer();
        let call_data = vec![0x2a, 0x00, 0xde, 0xad];
        let extensions = vec![
            ext("CheckNonce", &[0x04], &[]),
            ext("CheckGenesis", &[], &[9; 32]),
        ];

        let extrinsic = build_signed_extrinsic_v4(&signer, &call_data, &extensions);
        let (account, signature, tail) = split_v4(&extrinsic);

        // Body tail is Σextra ++ call_data (extra before call).
        let mut expected_tail = Vec::new();
        expected_tail.extend_from_slice(&extensions[0].extra);
        expected_tail.extend_from_slice(&extensions[1].extra);
        expected_tail.extend_from_slice(&call_data);
        assert_eq!(account, signer.account_id().0);
        assert_eq!(tail, expected_tail);

        // Signature verifies over call ++ Σextra ++ Σadditional (call first).
        let mut payload = call_data.clone();
        payload.extend_from_slice(&extensions[0].extra);
        payload.extend_from_slice(&extensions[1].extra);
        payload.extend_from_slice(&extensions[0].additional_signed);
        payload.extend_from_slice(&extensions[1].additional_signed);
        let public = PublicKey::from_bytes(&account).unwrap();
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    &payload,
                    &schnorrkel::Signature::from_bytes(&signature).unwrap()
                )
                .is_ok()
        );
    }

    #[test]
    fn v4_signer_payload_hashes_when_over_256_bytes() {
        let signer = test_signer();
        let call_data = vec![1u8; 200];
        // Push the payload (call ++ extra ++ additional) over 256 bytes.
        let extensions = vec![ext("Big", &[2u8; 60], &[3u8; 60])];
        let extrinsic = build_signed_extrinsic_v4(&signer, &call_data, &extensions);
        let (account, signature, _) = split_v4(&extrinsic);
        let public = PublicKey::from_bytes(&account).unwrap();
        let sig = schnorrkel::Signature::from_bytes(&signature).unwrap();

        let mut raw = call_data.clone();
        raw.extend_from_slice(&extensions[0].extra);
        raw.extend_from_slice(&extensions[0].additional_signed);
        assert!(raw.len() > 256);
        let hashed = sp_crypto_hashing::blake2_256(&raw);

        // Signs over the hash, not the raw concatenation.
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &hashed, &sig)
                .is_ok()
        );
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &raw, &sig)
                .is_err()
        );
    }

    #[test]
    fn v4_preserves_extension_order() {
        let signer = test_signer();
        let call_data = vec![0xff];
        // Deliberately not in sorted order; the assembler must not reorder.
        let extensions = vec![ext("B", &[0xbb], &[]), ext("A", &[0xaa], &[])];
        let (_, _, tail) = split_v4(&build_signed_extrinsic_v4(&signer, &call_data, &extensions));
        assert_eq!(tail, vec![0xbb, 0xaa, 0xff]);
    }

    /// Fixture extensions in metadata order, at their resting values.
    fn fixture_extensions(
        allowance_metadata: &AllowanceMetadata,
        state: &ChainState,
    ) -> Vec<TxPayloadExtension> {
        allowance_metadata
            .extension_ids()
            .into_iter()
            .zip(allowance_metadata.encode_signed_extensions(state))
            .map(|(id, encoded)| TxPayloadExtension {
                id: id.to_string(),
                extra: encoded.extra,
                additional_signed: encoded.additional_signed,
            })
            .collect()
    }

    /// Fixture extensions with `VerifyMultiSignature` omitted, i.e. the shape a
    /// caller sends when it wants the host to sign.
    fn fixture_signed_extensions(
        allowance_metadata: &AllowanceMetadata,
        state: &ChainState,
    ) -> Vec<TxPayloadExtension> {
        fixture_extensions(allowance_metadata, state)
            .into_iter()
            .filter(|extension| extension.id != "VerifyMultiSignature")
            .collect()
    }

    /// Resources.set_statement_store_account(period=7, seq=0, target=0),
    /// resolved by index against the checked-in V14 runtime metadata.
    fn fixture_call_data() -> Vec<u8> {
        let mut call_data = vec![0x3f, 0x0a];
        call_data.extend_from_slice(&7u32.to_le_bytes());
        call_data.extend_from_slice(&0u32.to_le_bytes());
        call_data.extend_from_slice(&[0u8; 32]);
        call_data
    }

    const FIXTURE_METADATA_BYTES: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/paseo-next-v2-metadata.scale"
    ));

    fn fixture_chain_state() -> ChainState {
        ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
            restrict_origins: false,
        }
    }

    #[test]
    fn v5_product_payload_signs_the_full_post_verify_implication() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let extensions = fixture_extensions(&allowance_metadata, &state);
        let verify_index = extensions
            .iter()
            .position(|extension| extension.id == "VerifyMultiSignature")
            .expect("fixture contains VerifyMultiSignature");

        let supplied: Vec<_> = extensions
            .iter()
            .filter(|extension| extension.id != "VerifyMultiSignature")
            .cloned()
            .collect();

        let call_data = fixture_call_data();
        let signer = test_signer();
        let transaction = build_signed_extrinsic_v5(
            &signer,
            state.genesis_hash,
            &call_data,
            &supplied,
            0,
            metadata.clone(),
        )
        .unwrap();

        assert_eq!(
            metadata
                .extrinsic()
                .transaction_extension_version_to_use_for_encoding(),
            0,
            "fixture uses transaction-extension pipeline version zero"
        );

        // FRAME implication oracle:
        // blake2_256(extension_version || call || suffix.extra || suffix.implicit),
        // where suffix is strictly after VerifyMultiSignature.
        let suffix = &extensions[verify_index + 1..];
        let mut implication = vec![0];
        implication.extend_from_slice(&call_data);
        for extension in suffix {
            implication.extend_from_slice(&extension.extra);
        }
        for extension in suffix {
            implication.extend_from_slice(&extension.additional_signed);
        }
        let signer_payload = sp_crypto_hashing::blake2_256(&implication);

        let mut inner = &transaction[..];
        let encoded_len = Compact::<u32>::decode(&mut inner).unwrap().0 as usize;
        assert_eq!(inner.len(), encoded_len);
        assert_eq!(inner[0], 0x45, "V5 General version byte");
        assert_eq!(inner[1], 0, "transaction-extension pipeline version");

        let verify_offset = 2 + extensions[..verify_index]
            .iter()
            .map(|extension| extension.extra.len())
            .sum::<usize>();
        assert_eq!(inner[verify_offset], 1, "VerifySignature::Signed variant");
        assert_eq!(
            inner[verify_offset + 1],
            1,
            "MultiSignature::Sr25519 variant"
        );
        let signature: [u8; 64] = inner[verify_offset + 2..verify_offset + 66]
            .try_into()
            .unwrap();
        let account: [u8; 32] = inner[verify_offset + 66..verify_offset + 98]
            .try_into()
            .unwrap();
        assert_eq!(account, signer.account_id().0);

        let mut expected_tail = Vec::new();
        for extension in suffix {
            expected_tail.extend_from_slice(&extension.extra);
        }
        expected_tail.extend_from_slice(&call_data);
        assert_eq!(&inner[verify_offset + 98..], expected_tail);

        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        let public = PublicKey::from_bytes(&account).unwrap();
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &signer_payload, &signature)
                .is_ok(),
            "embedded signature verifies over the complete V5 implication"
        );

        // This is the exact historical failure mode in frame-decode: the
        // Verify extension cleared the base along with preceding extensions.
        let mut cleared_base = Vec::new();
        for extension in suffix {
            cleared_base.extend_from_slice(&extension.extra);
        }
        for extension in suffix {
            cleared_base.extend_from_slice(&extension.additional_signed);
        }
        assert!(
            public
                .verify_simple(
                    SR25519_SIGNING_CONTEXT,
                    &sp_crypto_hashing::blake2_256(&cleared_base),
                    &signature,
                )
                .is_err(),
            "signature must not verify against the old base-clearing payload"
        );
    }

    #[test]
    fn v5_caller_supplied_verify_signature_is_encoded_verbatim_and_unsigned() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let extensions = fixture_extensions(&allowance_metadata, &state);
        let verify_index = extensions
            .iter()
            .position(|extension| extension.id == "VerifyMultiSignature")
            .expect("fixture contains VerifyMultiSignature");
        assert_eq!(
            extensions[verify_index].extra,
            vec![0x00],
            "fixture supplies the Disabled variant"
        );

        let call_data = fixture_call_data();
        let signer = test_signer();
        let transaction = build_signed_extrinsic_v5(
            &signer,
            state.genesis_hash,
            &call_data,
            &extensions,
            0,
            metadata,
        )
        .unwrap();

        let mut inner = &transaction[..];
        let encoded_len = Compact::<u32>::decode(&mut inner).unwrap().0 as usize;
        assert_eq!(inner.len(), encoded_len);
        assert_eq!(inner[0], 0x45, "V5 General version byte");
        assert_eq!(inner[1], 0, "transaction-extension pipeline version");

        let mut expected = vec![0x45, 0];
        for extension in &extensions {
            expected.extend_from_slice(&extension.extra);
        }
        expected.extend_from_slice(&call_data);
        assert_eq!(inner, expected);

        let verify_offset = 2 + extensions[..verify_index]
            .iter()
            .map(|extension| extension.extra.len())
            .sum::<usize>();
        assert_eq!(
            inner[verify_offset], 0,
            "VerifySignature::Disabled variant survives"
        );
        assert!(
            !inner
                .windows(32)
                .any(|window| window == signer.account_id().0),
            "an unsigned transaction carries no signer account"
        );
    }

    /// The proof-authorized path must lay the body out exactly like the
    /// hand-rolled assembler behind chain-accepted allowance extrinsics. Both
    /// sides source values from `encode_signed_extensions`, so this pins the
    /// layout — preamble, ordering, no spliced signature — not the values.
    #[test]
    fn v5_proof_authorized_bytes_match_the_statement_allowance_assembler() {
        use crate::runtime::statement_allowance::extrinsic::{
            build_long_term_storage_extra, build_unsigned_extrinsic,
        };

        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let call_data = fixture_call_data();

        let as_resources_extra = build_long_term_storage_extra(
            &allowance_metadata,
            &[0xEE; 785],
            3,
            9,
            PersonhoodCollection::LitePeople,
        )
        .unwrap();
        let expected =
            build_unsigned_extrinsic(&allowance_metadata, &state, &call_data, &as_resources_extra)
                .unwrap();

        let as_resources_index = allowance_metadata.as_resources_index().unwrap();
        let mut extensions = fixture_extensions(&allowance_metadata, &state);
        extensions[as_resources_index]
            .extra
            .clone_from(&as_resources_extra);

        let transaction = build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &call_data,
            &extensions,
            0,
            metadata,
        )
        .unwrap();

        assert_eq!(transaction, expected);
    }

    /// A caller may also supply a `Signed` value; the host must not replace it.
    #[test]
    fn v5_caller_supplied_signed_variant_survives() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let mut extensions = fixture_extensions(&allowance_metadata, &state);
        let verify_index = extensions
            .iter()
            .position(|extension| extension.id == "VerifyMultiSignature")
            .unwrap();

        // Signed{signature: Sr25519(..), account: ..} against this runtime's
        // variant indices: Disabled is 0, so Signed is 1.
        let mut signed = vec![0x01, 0x01];
        signed.extend_from_slice(&[0xAA; 64]);
        signed.extend_from_slice(&[0xBB; 32]);
        extensions[verify_index].extra.clone_from(&signed);

        let transaction = build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &extensions,
            0,
            metadata,
        )
        .unwrap();

        let mut inner = &transaction[..];
        let _ = Compact::<u32>::decode(&mut inner).unwrap();
        let verify_offset = 2 + extensions[..verify_index]
            .iter()
            .map(|extension| extension.extra.len())
            .sum::<usize>();
        assert_eq!(
            &inner[verify_offset..verify_offset + signed.len()],
            &signed[..]
        );
    }

    /// An id the runtime does not declare is rejected rather than dropped.
    #[test]
    fn v5_rejects_an_extension_the_runtime_does_not_declare() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let extensions = fixture_extensions(&allowance_metadata, &state);

        // The realistic confusion is the near-miss — the pallet is
        // `VerifySignature`, the extension is `VerifyMultiSignature` — but the
        // rule is not special to it, so a plain stale id must fail too.
        for bad_id in ["VerifySignature", "CheckNonsense"] {
            let mut extensions = extensions.clone();
            extensions
                .iter_mut()
                .find(|extension| extension.id == "VerifyMultiSignature")
                .unwrap()
                .id = bad_id.to_string();

            let error = build_signed_extrinsic_v5(
                &test_signer(),
                state.genesis_hash,
                &fixture_call_data(),
                &extensions,
                0,
                metadata.clone(),
            )
            .unwrap_err();
            let error = error.to_string();
            assert!(
                error.contains(&format!(
                    "transaction extension {bad_id:?} is not declared by the runtime"
                )) && error.contains("VerifyMultiSignature"),
                "{error}"
            );
        }
    }

    /// The host-owned `VerifyMultiSignature`, built the way production does.
    fn host_owned_verify_signature(
        metadata: &ArcMetadata,
        genesis_hash: [u8; 32],
    ) -> VerifySignature<SubstrateConfig> {
        let client_state = ClientState {
            genesis_hash: H256(genesis_hash),
            spec_version: 0,
            transaction_version: 0,
            metadata: metadata.clone(),
        };
        <VerifySignature<SubstrateConfig> as SubxtTransactionExtension<SubstrateConfig>>::new(
            &client_state,
            (),
        )
        .unwrap()
    }

    /// A short or empty implicit after the authorization cut is signed verbatim
    /// into the payload, so it must fail here rather than as a BadProof.
    #[test]
    fn v5_rejects_a_truncated_signed_implicit() {
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let signed = fixture_signed_extensions(&allowance_metadata, &state);

        // These four are the fixture's non-empty implicits after the cut.
        for id in [
            "CheckSpecVersion",
            "CheckTxVersion",
            "CheckGenesis",
            "CheckMortality",
        ] {
            for (implicit, expected) in [
                (Vec::new(), "does not decode"),
                (vec![0xAB], "does not decode"),
            ] {
                let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
                let mut extensions = signed.clone();
                extensions
                    .iter_mut()
                    .find(|extension| extension.id == id)
                    .unwrap()
                    .additional_signed = implicit.clone();

                let error = build_signed_extrinsic_v5(
                    &test_signer(),
                    state.genesis_hash,
                    &fixture_call_data(),
                    &extensions,
                    0,
                    metadata,
                )
                .unwrap_err()
                .to_string();
                assert!(
                    error.contains(&format!("{id} implicit")) && error.contains(expected),
                    "{id} implicit={implicit:?} gave {error:?}"
                );
            }
        }
    }

    /// Post-cut values are hashed into the signer payload alongside the
    /// implicits, so a malformed one there is signed just as blindly.
    #[test]
    fn v5_rejects_a_malformed_post_cut_value() {
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        for (id, extra, expected) in [
            ("CheckNonce", vec![0x00, 0xFF], "has 1 trailing byte(s)"),
            ("CheckMortality", Vec::new(), "does not decode"),
            ("ChargeAssetTxPayment", vec![0x00], "does not decode"),
        ] {
            let mut extensions = fixture_signed_extensions(&allowance_metadata, &state);
            extensions
                .iter_mut()
                .find(|extension| extension.id == id)
                .unwrap()
                .extra = extra.clone();

            let error = build_signed_extrinsic_v5(
                &test_signer(),
                state.genesis_hash,
                &fixture_call_data(),
                &extensions,
                0,
                Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc(),
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains(&format!("{id} value")) && error.contains(expected),
                "{id} extra={extra:?} gave {error:?}"
            );
        }
    }

    /// The value guard must pass a real proof through: `AsResources` carrying a
    /// ring-VRF claim is the whole point of the caller-authorized path.
    #[test]
    fn v5_accepts_a_real_as_resources_proof_value() {
        use crate::runtime::statement_allowance::extrinsic::build_long_term_storage_extra;

        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let mut extensions = fixture_signed_extensions(&allowance_metadata, &state);
        extensions
            .iter_mut()
            .find(|extension| extension.id == "AsResources")
            .unwrap()
            .extra = build_long_term_storage_extra(
            &allowance_metadata,
            &[0xEE; 785],
            3,
            9,
            PersonhoodCollection::LitePeople,
        )
        .unwrap();

        build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &extensions,
            0,
            Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc(),
        )
        .expect("a well-formed proof value must pass the guard");
    }

    /// Every reader takes the first entry for a name, so a repeat would be
    /// dropped in silence — and on the authorization slot it would also decide
    /// whether the host signs.
    #[test]
    fn v5_rejects_a_duplicate_extension_id() {
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let mut extensions = fixture_signed_extensions(&allowance_metadata, &state);
        let mut repeated = extensions[0].clone();
        repeated.extra = vec![0xAB];
        extensions.push(repeated);
        let repeated_id = extensions[0].id.clone();

        let error = build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &extensions,
            0,
            Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(&format!("{repeated_id:?} is listed more than once")),
            "{error}"
        );
    }

    /// An empty-typed implicit is filtered out by `frame-decode` before it asks
    /// for bytes, so spurious bytes there are never signed and must not be
    /// rejected.
    #[test]
    fn v5_accepts_spurious_bytes_for_an_empty_typed_implicit() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let mut extensions = fixture_signed_extensions(&allowance_metadata, &state);
        let nonce = extensions
            .iter_mut()
            .find(|extension| extension.id == "CheckNonce")
            .unwrap();
        assert!(
            nonce.additional_signed.is_empty(),
            "CheckNonce's implicit type is empty in this fixture"
        );
        nonce.additional_signed = vec![0xAB, 0xCD];

        build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &extensions,
            0,
            metadata,
        )
        .expect("an unread implicit must not fail the build");
    }

    /// Implicits are never read on the caller-supplied branch, so bytes there
    /// must not be rejected — the V5 body carries values only, and no signer
    /// payload is computed.
    #[test]
    fn v5_ignores_implicits_when_the_caller_owns_the_authorization() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let mut extensions = fixture_extensions(&allowance_metadata, &state);
        for extension in &mut extensions {
            extension.additional_signed = vec![0xAB; 3];
        }

        let transaction = build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &extensions,
            0,
            metadata,
        )
        .unwrap();

        let mut expected = vec![0x45, 0];
        for extension in &extensions {
            expected.extend_from_slice(&extension.extra);
        }
        expected.extend_from_slice(&fixture_call_data());
        let mut inner = &transaction[..];
        let _ = Compact::<u32>::decode(&mut inner).unwrap();
        assert_eq!(inner, expected, "implicit bytes must not reach the body");
    }

    /// A V16 blob built from the V14 fixture, declaring the extension pipeline
    /// at two versions so the cross-version union has something to span.
    /// Version 0 declares every extension the fixture carries; version 1 — the
    /// one subxt encodes with, being the highest — declares all but `omitted`.
    ///
    /// Subxt synthesises `{0: all}` for any V14 or V15 blob, so a single-version
    /// fixture cannot tell a union apart from a lookup at the encoding version.
    /// Only the parts this module reads are carried over: the type registry,
    /// each pallet's index and call type, and the extension list. Call
    /// arguments stay opaque, so their field types are never consulted.
    fn two_version_metadata(omitted: &[&str]) -> ArcMetadata {
        use std::collections::BTreeMap;
        use subxt::ext::frame_metadata::{META_RESERVED, v16};

        let RuntimeMetadata::V14(v14) =
            RuntimeMetadataPrefixed::decode(&mut &FIXTURE_METADATA_BYTES[..])
                .unwrap()
                .1
        else {
            panic!("fixture is not V14");
        };

        // `UncheckedExtrinsic<Address, Call, Signature, Extra>`: V14 leaves these
        // to be read off the type params, V16 names them outright.
        let extrinsic_ty = v14.types.resolve(v14.extrinsic.ty.id).unwrap();
        let param = |name: &str| {
            extrinsic_ty
                .type_params
                .iter()
                .find(|param| param.name == name)
                .and_then(|param| param.ty)
                .unwrap_or_else(|| panic!("extrinsic type has no {name} parameter"))
        };
        let (address_ty, call_ty, signature_ty) =
            (param("Address"), param("Call"), param("Signature"));

        let transaction_extensions: Vec<
            v16::TransactionExtensionMetadata<scale_info::form::PortableForm>,
        > = v14
            .extrinsic
            .signed_extensions
            .iter()
            .map(|extension| v16::TransactionExtensionMetadata {
                identifier: extension.identifier.clone(),
                ty: extension.ty,
                implicit: extension.additional_signed,
            })
            .collect();
        let every_extension = (0..transaction_extensions.len() as u32)
            .map(Compact)
            .collect();
        let encoding_version_extensions = transaction_extensions
            .iter()
            .enumerate()
            .filter(|(_, extension)| !omitted.contains(&extension.identifier.as_str()))
            .map(|(index, _)| Compact(index as u32))
            .collect();

        let pallets = v14
            .pallets
            .iter()
            .map(|pallet| v16::PalletMetadata {
                name: pallet.name.clone(),
                storage: None,
                calls: pallet.calls.as_ref().map(|calls| v16::PalletCallMetadata {
                    ty: calls.ty,
                    deprecation_info: v16::EnumDeprecationInfo::nothing_deprecated(),
                }),
                event: None,
                constants: Vec::new(),
                error: None,
                associated_types: Vec::new(),
                view_functions: Vec::new(),
                index: pallet.index,
                docs: Vec::new(),
                deprecation_info: v16::ItemDeprecationInfo::NotDeprecated,
            })
            .collect();

        let metadata = v16::RuntimeMetadataV16 {
            types: v14.types,
            pallets,
            extrinsic: v16::ExtrinsicMetadata {
                versions: vec![4, 5],
                address_ty,
                call_ty,
                signature_ty,
                transaction_extensions_by_version: BTreeMap::from_iter([
                    (0, every_extension),
                    (1, encoding_version_extensions),
                ]),
                transaction_extensions,
            },
            apis: Vec::new(),
            outer_enums: v16::OuterEnums {
                call_enum_ty: call_ty,
                event_enum_ty: call_ty,
                error_enum_ty: call_ty,
            },
            custom: v16::CustomMetadata {
                map: Default::default(),
            },
        };
        let bytes = RuntimeMetadataPrefixed(META_RESERVED, RuntimeMetadata::V16(metadata)).encode();
        Metadata::decode_from(&bytes).unwrap().arc()
    }

    /// An id the encoding version omits but another version declares is a
    /// product reading a different view of the same runtime, not a mistake. The
    /// encoder never asks for its bytes, so the build stands.
    #[test]
    fn v5_accepts_an_id_only_another_pipeline_version_declares() {
        let metadata = two_version_metadata(&["RestrictOrigins"]);
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let encoding_version = metadata
            .extrinsic()
            .transaction_extension_version_to_use_for_encoding();
        assert_eq!(
            encoding_version, 1,
            "the highest version is the one encoded"
        );
        assert!(
            !metadata
                .extrinsic_extension_info(Some(encoding_version))
                .unwrap()
                .extension_ids
                .iter()
                .any(|declared| declared.name == "RestrictOrigins"),
            "the encoding version must omit the id under test"
        );

        build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &fixture_signed_extensions(&allowance_metadata, &state),
            encoding_version,
            metadata,
        )
        .expect("an id declared at another version is surplus, not fatal");
    }

    /// Declining to sign is the caller's whole intent, so an encoding version
    /// with nowhere to put their bytes must not quietly drop them.
    #[test]
    fn v5_rejects_a_supplied_verify_signature_the_encoding_version_omits() {
        let metadata = two_version_metadata(&["VerifyMultiSignature"]);
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let error = build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &fixture_extensions(&allowance_metadata, &state),
            1,
            metadata,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("does not declare VerifyMultiSignature")
                && error.contains("supplied value cannot authorize"),
            "{error}"
        );
    }

    /// The mirror case: nothing supplies the slot and the encoding version has
    /// none, so the host's signature would be computed and then dropped by the
    /// body encoder, yielding a transaction with no authorization at all.
    #[test]
    fn v5_rejects_a_host_signed_build_with_no_authorization_slot() {
        let metadata = two_version_metadata(&["VerifyMultiSignature"]);
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let error = build_signed_extrinsic_v5(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &fixture_signed_extensions(&allowance_metadata, &state),
            1,
            metadata,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("does not declare VerifyMultiSignature")
                && error.contains("host cannot authorize"),
            "{error}"
        );
    }

    /// Malformed authorization bytes must fail here, not shift the body.
    #[test]
    fn v5_rejects_malformed_verify_signature_bytes() {
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        for (extra, expected) in [
            (vec![], "Ran out of data"),
            (vec![0x00, 0xff, 0xff], "has 2 trailing byte(s)"),
            // A truncated `Signed`, distinct from the empty case above.
            (vec![0x01, 0x01], "Not enough data"),
            (vec![0x01, 0xff], "Could not find variant with index 255"),
        ] {
            let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
            let mut extensions = fixture_extensions(&allowance_metadata, &state);
            extensions
                .iter_mut()
                .find(|extension| extension.id == "VerifyMultiSignature")
                .unwrap()
                .extra = extra.clone();

            let error = build_signed_extrinsic_v5(
                &test_signer(),
                state.genesis_hash,
                &fixture_call_data(),
                &extensions,
                0,
                metadata,
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains(expected),
                "extra={extra:?} gave {error:?}, wanted {expected:?}"
            );
        }
    }

    /// Only `VerifyMultiSignature` may claim the V5 authorization cut, whether
    /// the host or the caller owns its bytes.
    #[test]
    fn only_verify_multi_signature_is_an_authorization_extension() {
        fn is_authorization<R: TypeResolver>(
            extensions: &PreencodedExtensions<'_>,
            name: &str,
            _types: &R,
        ) -> bool
        where
            VerifySignature<SubstrateConfig>: FrameTransactionExtension<R>,
        {
            FrameTransactionExtensions::<R>::is_authorization_extension(extensions, name)
        }

        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();
        let supplied = fixture_extensions(&allowance_metadata, &state);

        for host_owned in [true, false] {
            let verify_signature =
                host_owned.then(|| host_owned_verify_signature(&metadata, state.genesis_hash));
            let extensions = PreencodedExtensions {
                supplied: &supplied,
                verify_signature,
            };
            assert!(
                is_authorization(&extensions, "VerifyMultiSignature", metadata.types()),
                "host_owned={host_owned}"
            );
            // AsResources in particular: its ring-VRF implication genuinely
            // covers a later span, but claiming the cut here would move it.
            for other in supplied
                .iter()
                .filter(|extension| extension.id != "VerifyMultiSignature")
            {
                assert!(
                    !is_authorization(&extensions, &other.id, metadata.types()),
                    "{} must not claim the authorization cut",
                    other.id
                );
            }
        }
    }

    /// Version byte and transaction extension version of an encoded
    /// transaction, read past its length prefix.
    fn envelope(transaction: &[u8]) -> (u8, u8) {
        let mut inner = transaction;
        Compact::<u32>::decode(&mut inner).unwrap();
        (inner[0], inner[1])
    }

    /// Version 0 builds V5 whenever it can carry the host's signature.
    #[test]
    fn tx_ext_version_zero_builds_v5_when_version_zero_verifies_signatures() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let transaction = build_signed_transaction(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &fixture_signed_extensions(&allowance_metadata, &state),
            0,
            metadata,
        )
        .unwrap();

        assert_eq!(envelope(&transaction), (0x45, 0));
    }

    /// Without a signature slot in version 0 a V5 transaction could not be
    /// authorized, so zero falls back to a signed V4 transaction.
    #[test]
    fn tx_ext_version_zero_builds_v4_when_version_zero_has_no_signature_slot() {
        let state = bulletin_chain_state();
        let extensions = vec![ext("CheckNonce", &[0x00], &[])];

        let transaction = build_signed_transaction(
            &test_signer(),
            state.genesis_hash,
            &[0x00, 0x00],
            &extensions,
            0,
            state.metadata,
        )
        .unwrap();

        assert_eq!(envelope(&transaction).0, 0x84);
    }

    /// The encoded transaction extension version is the one the caller names,
    /// not the one the runtime would pick for itself.
    #[test]
    fn tx_ext_version_is_the_encoded_transaction_extension_version() {
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let versions: Vec<_> = [0, 1]
            .into_iter()
            .map(|tx_ext_version| {
                let transaction = build_signed_transaction(
                    &test_signer(),
                    state.genesis_hash,
                    &fixture_call_data(),
                    &fixture_signed_extensions(&allowance_metadata, &state),
                    tx_ext_version,
                    two_version_metadata(&["RestrictOrigins"]),
                )
                .unwrap();
                envelope(&transaction)
            })
            .collect();

        assert_eq!(versions, vec![(0x45, 0), (0x45, 1)]);
    }

    /// An undeclared version has no extension set to encode against, so it is
    /// refused rather than built on another version.
    #[test]
    fn tx_ext_version_the_runtime_does_not_declare_is_not_supported() {
        let metadata = Metadata::decode_from(FIXTURE_METADATA_BYTES).unwrap().arc();
        let allowance_metadata = AllowanceMetadata::decode(FIXTURE_METADATA_BYTES).unwrap();
        let state = fixture_chain_state();

        let error = build_signed_transaction(
            &test_signer(),
            state.genesis_hash,
            &fixture_call_data(),
            &fixture_signed_extensions(&allowance_metadata, &state),
            5,
            metadata,
        )
        .unwrap_err()
        .to_string();

        assert_eq!(
            error,
            "unsupported tx_ext_version 5; the runtime declares transaction extension versions [0]"
        );
    }
}
