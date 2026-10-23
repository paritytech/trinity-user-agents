//! Builder tests against the fixture runtimes. Each byte-level test compares
//! with an independent builder: the statement allowance assembler, its
//! implication, or subxt's own V4 transaction.

use std::sync::{Arc, Mutex};

use parity_scale_codec::{Compact, Decode};

use subxt::{
	Metadata,
	config::{
		DefaultExtrinsicParamsBuilder, substrate::SubstrateConfig,
		transaction_extensions::VerifySignatureDetails,
	},
	dynamic::{Value, tx},
	ext::{
		frame_decode::extrinsics::{ExtrinsicTypeInfo, encode_call_data},
		scale_decode::DecodeAsType,
	},
	tx::Signer,
	utils::{AccountId32, H256, MultiSignature},
};

use super::{
	ExtensionError, ExtrinsicBuildError, ExtrinsicVersion, InheritedImplication, RawExtension,
	RestrictOrigins, TransactionExtension, VersionChoice,
	builder::{RuntimeAtBirth, account_needing_nonce, assemble, start_build},
	version::choose_version,
};
use crate::{
	chain::HashAndNumber,
	host_internal::extrinsic::{
		Sr25519Signer,
		tests::{bulletin_chain_state, split_v4},
	},
	runtime::statement_allowance::{
		collection::PersonhoodCollection,
		extension::{
			ChainState, Metadata as AllowanceMetadata, build_proof_message_after_extension,
		},
		extrinsic::{build_long_term_storage_extra, build_unsigned_extrinsic},
	},
};

const PEOPLE: &[u8] = include_bytes!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/tests/fixtures/paseo-next-v2-metadata-v16.scale"
));
const ASSET_HUB: &[u8] = include_bytes!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/tests/fixtures/paseo-next-asset-hub-3004000-metadata.scale"
));

const GENESIS: [u8; 32] = [0xab; 32];
const BIRTH: HashAndNumber = HashAndNumber { hash: H256([0x11; 32]), number: 100 };

fn runtime(metadata_bytes: &[u8]) -> RuntimeAtBirth {
	RuntimeAtBirth {
		genesis: H256(GENESIS),
		birth: BIRTH,
		spec_version: 1_000_000,
		transaction_version: 1,
		metadata: Metadata::decode_from(metadata_bytes).unwrap().arc(),
	}
}

fn test_signer() -> Sr25519Signer {
	let keypair = schnorrkel::MiniSecretKey::from_bytes(&[3; 32])
		.unwrap()
		.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
	Sr25519Signer::from_keypair(&keypair)
}

fn remark_call(metadata_bytes: &[u8]) -> Vec<u8> {
	let metadata = Metadata::decode_from(metadata_bytes).unwrap();
	let remark = tx("System", "remark", vec![Value::from_bytes([1, 2, 3])]);
	encode_call_data("System", "remark", remark.call_data(), &metadata, metadata.types()).unwrap()
}

fn allowance_state() -> ChainState {
	ChainState {
		spec_version: 1_000_000,
		transaction_version: 1,
		genesis_hash: GENESIS,
		nonce: 0,
		restrict_origins: false,
	}
}

/// `CheckMortality` as the allowance assembler writes it: immortal, with the
/// genesis hash as its implicit.
fn immortal() -> RawExtension {
	RawExtension::new("CheckMortality", vec![0x00], GENESIS.to_vec())
}

fn long_term_storage_extra(allowance: &AllowanceMetadata) -> Vec<u8> {
	build_long_term_storage_extra(allowance, &[0xEE; 785], 3, 9, PersonhoodCollection::LitePeople)
		.unwrap()
}

/// Records the implication it was given, then answers with fixed bytes.
struct RecordingExtension {
	name: &'static str,
	explicit: Vec<u8>,
	seen: Arc<Mutex<Option<InheritedImplication>>>,
}

impl TransactionExtension for RecordingExtension {
	type Implicit = ();
	type Explicit = subxt::utils::Encoded;

	fn name(&self) -> &str {
		self.name
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<subxt::utils::Encoded, ExtensionError> {
		*self.seen.lock().unwrap() = Some(implication.clone());
		Ok(subxt::utils::Encoded(self.explicit.clone()))
	}
}

/// The proof-authorized bytes are what statement allowance submits today, so
/// moving it onto the builder must not change a byte apart from the era.
#[test]
fn an_authorized_extrinsic_matches_the_allowance_assembler() {
	let allowance = AllowanceMetadata::decode(PEOPLE).unwrap();
	let call = remark_call(PEOPLE);
	let extra = long_term_storage_extra(&allowance);
	let expected = build_unsigned_extrinsic(&allowance, &allowance_state(), &call, &extra).unwrap();

	let mut builder = start_build(runtime(PEOPLE));
	builder
		.with_call_bytes(call)
		.with_extension(RawExtension::new("AsResources", extra, Vec::new()))
		.with_extension(immortal())
		.with_extrinsic_version(ExtrinsicVersion::newest_declaring("AsResources"));
	let built = assemble(builder, None, None).unwrap();

	assert_eq!(built.extrinsic.bytes(), expected);
}

/// A ring proof covers exactly the extensions after its authorizer; one more
/// or one fewer and the node rejects the proof.
#[test]
fn an_authorizer_sees_the_implication_of_the_extensions_after_it() {
	let allowance = AllowanceMetadata::decode(PEOPLE).unwrap();
	let call = remark_call(PEOPLE);
	let expected =
		build_proof_message_after_extension(&allowance, &call, &allowance_state(), "AsResources")
			.unwrap();
	let seen = Arc::new(Mutex::new(None));

	let mut builder = start_build(runtime(PEOPLE));
	builder
		.with_call_bytes(call)
		.with_extension(RecordingExtension {
			name: "AsResources",
			explicit: vec![0x00],
			seen: seen.clone(),
		})
		.with_extension(immortal())
		.with_extrinsic_version(ExtrinsicVersion::newest_declaring("AsResources"));
	assemble(builder, None, None).unwrap();

	assert_eq!(seen.lock().unwrap().as_ref().map(InheritedImplication::hash), Some(expected));
}

/// Signs `call` on Bulletin through subxt, reusing `signature` so the bytes
/// are comparable, and returns subxt's extrinsic and signer payload.
fn subxt_bulletin_v4(
	nonce: u64,
	account: &AccountId32,
	signature: &MultiSignature,
) -> (Vec<u8>, Vec<u8>) {
	let state = bulletin_chain_state();
	let client = state.client_at(BIRTH.number).unwrap();
	let remark = tx("System", "remark", vec![Value::from_bytes([1, 2, 3])]);
	let params = DefaultExtrinsicParamsBuilder::<SubstrateConfig>::new()
		.mortal_from_unchecked(64, BIRTH.number, BIRTH.hash)
		.nonce(nonce)
		.build();
	let mut signable = client.tx().create_v4_signable_offline(&remark, params).unwrap();
	let payload = signable.signer_payload().unwrap();
	let signed = signable.sign_with_account_and_signature(account, signature).unwrap();
	(signed.into_encoded(), payload)
}

fn bulletin_runtime() -> RuntimeAtBirth {
	let state = bulletin_chain_state();
	RuntimeAtBirth {
		genesis: H256(state.genesis_hash),
		birth: BIRTH,
		spec_version: state.spec_version,
		transaction_version: state.transaction_version,
		metadata: state.metadata,
	}
}

fn build_bulletin_remark(
	signer: &Sr25519Signer,
	nonce_override: Option<u64>,
	account_nonce: Option<u64>,
) -> Vec<u8> {
	let mut builder = start_build(bulletin_runtime());
	builder
		.with_call(tx("System", "remark", vec![Value::from_bytes([1, 2, 3])]))
		.with_signer(signer);
	if let Some(nonce) = nonce_override {
		builder.with_nonce(nonce);
	}
	assemble(builder, None, account_nonce).unwrap().extrinsic.bytes().to_vec()
}

/// Bulletin builds through subxt today; the builder must produce the same
/// V4 extrinsic and a signature over subxt's own payload.
#[test]
fn a_signed_v4_extrinsic_matches_subxt() {
	let signer = test_signer();
	let built = build_bulletin_remark(&signer, None, Some(7));
	let (account, signature, _) = split_v4(&built);
	let signature = MultiSignature::Sr25519(signature);

	let (expected, payload) = subxt_bulletin_v4(7, &AccountId32(account), &signature);

	assert_eq!(built, expected);
	assert!(verifies(&signer, &payload, &signature));
}

/// A nonce the caller sets wins over the one read from the pool.
#[test]
fn a_caller_nonce_overrides_the_account_nonce() {
	let signer = test_signer();
	let built = build_bulletin_remark(&signer, Some(5), Some(7));
	let (account, signature, _) = split_v4(&built);

	let (expected, _) =
		subxt_bulletin_v4(5, &AccountId32(account), &MultiSignature::Sr25519(signature));

	assert_eq!(built, expected);
}

fn verifies(signer: &Sr25519Signer, payload: &[u8], signature: &MultiSignature) -> bool {
	let MultiSignature::Sr25519(bytes) = signature else {
		return false;
	};
	let public = schnorrkel::PublicKey::from_bytes(&signer.account_id().0).unwrap();
	let signature = schnorrkel::Signature::from_bytes(bytes).unwrap();
	public
		.verify_simple(
			crate::host_logic::product_account::SR25519_SIGNING_CONTEXT,
			payload,
			&signature,
		)
		.is_ok()
}

/// The pool nonce is read only when the build signs and the caller set none.
#[test]
fn only_a_signed_build_without_a_nonce_reads_the_account_nonce() {
	let signer = test_signer();
	let mut signed = start_build(bulletin_runtime());
	signed.with_signer(&signer);
	let mut pinned = start_build(bulletin_runtime());
	pinned.with_signer(&signer).with_nonce(1);
	let unsigned = start_build(bulletin_runtime());

	assert_eq!(account_needing_nonce(&signed), Some(signer.account_id()));
	assert_eq!(account_needing_nonce(&pinned), None);
	assert_eq!(account_needing_nonce(&unsigned), None);
}

/// The host's map decides the default exactly as the native hosts do.
#[test]
fn the_host_map_decides_the_default_version() {
	let people = Metadata::decode_from(PEOPLE).unwrap();
	let asset_hub = Metadata::decode_from(ASSET_HUB).unwrap();

	assert_eq!(choose_version(None, None, true, &people).unwrap(), ExtrinsicVersion::V4);
	assert_eq!(
		choose_version(None, Some(0), true, &people).unwrap(),
		ExtrinsicVersion::V5 { extension_version: 0 }
	);
	// Asset Hub's pipeline 0 has no `VerifyMultiSignature`.
	assert_eq!(choose_version(None, Some(0), true, &asset_hub).unwrap(), ExtrinsicVersion::V4);
	assert_eq!(
		choose_version(None, Some(0), false, &asset_hub).unwrap(),
		ExtrinsicVersion::V5 { extension_version: 0 }
	);
}

/// On Asset Hub 3004000 only pipeline 1 carries the authorizers, which is
/// why authorized callers pin by name rather than by number.
#[test]
fn newest_declaring_picks_the_pipeline_that_lists_the_extension() {
	let asset_hub = Metadata::decode_from(ASSET_HUB).unwrap();
	let pgas = Some(ExtrinsicVersion::newest_declaring("AsPgas"));
	let missing = Some(ExtrinsicVersion::newest_declaring("AsNothing"));

	assert_eq!(
		choose_version(pgas, None, false, &asset_hub).unwrap(),
		ExtrinsicVersion::V5 { extension_version: 1 }
	);
	// Both pipelines list `CheckNonce`; the newer one wins.
	assert_eq!(
		choose_version(
			Some(ExtrinsicVersion::newest_declaring("CheckNonce")),
			None,
			false,
			&asset_hub
		)
		.unwrap(),
		ExtrinsicVersion::V5 { extension_version: 1 }
	);
	assert!(matches!(
		choose_version(missing, None, false, &asset_hub),
		Err(ExtrinsicBuildError::NoPipelineDeclares { extension }) if extension == "AsNothing"
	));
}

fn build_error(
	metadata_bytes: &[u8],
	signer: Option<&Sr25519Signer>,
	setup: impl FnOnce(&mut super::ExtrinsicBuilder<'_>),
) -> ExtrinsicBuildError {
	let mut builder = start_build(runtime(metadata_bytes));
	builder.with_call_bytes(remark_call(metadata_bytes));
	if let Some(signer) = signer {
		builder.with_signer(signer);
	}
	setup(&mut builder);
	assemble(builder, None, Some(0)).unwrap_err()
}

#[test]
fn a_v4_extrinsic_without_a_signer_is_refused() {
	let error = build_error(PEOPLE, None, |_| {});

	assert!(matches!(error, ExtrinsicBuildError::UnsignedV4));
}

/// Pinning a pipeline never drops the signature in silence.
#[test]
fn a_signer_on_a_pipeline_without_a_signature_slot_is_refused() {
	let signer = test_signer();
	let v5_zero = VersionChoice::Exact(ExtrinsicVersion::V5 { extension_version: 0 });
	let error = build_error(ASSET_HUB, Some(&signer), |builder| {
		builder.with_extrinsic_version(v5_zero);
	});

	assert!(matches!(error, ExtrinsicBuildError::NoSignatureSlot { extension_version: 0 }));
}

#[test]
fn an_extension_the_pipeline_does_not_list_is_refused() {
	let pgas_v0 = VersionChoice::Exact(ExtrinsicVersion::V5 { extension_version: 0 });
	let error = build_error(ASSET_HUB, None, |builder| {
		builder
			.with_extension(RawExtension::new("AsPgas", vec![0x00], Vec::new()))
			.with_extrinsic_version(pgas_v0);
	});

	assert!(
		matches!(error, ExtrinsicBuildError::UndeclaredExtension { extension, .. } if extension == "AsPgas")
	);
}

#[test]
fn an_extension_supplied_twice_is_refused() {
	let signer = test_signer();
	let error = build_error(PEOPLE, Some(&signer), |builder| {
		builder
			.with_extension(RestrictOrigins(true))
			.with_extension(RestrictOrigins(false));
	});

	assert!(
		matches!(error, ExtrinsicBuildError::DuplicateExtension { extension } if extension == "RestrictOrigins")
	);
}

/// A value that does not decode as its type would sign one thing and send
/// another; it fails here with the bytes, not at the node.
#[test]
fn a_value_that_does_not_match_its_type_is_refused() {
	let signer = test_signer();
	let error = build_error(PEOPLE, Some(&signer), |builder| {
		builder.with_extension(RawExtension::new("RestrictOrigins", vec![0x00, 0x00], Vec::new()));
	});

	assert!(matches!(
		error,
		ExtrinsicBuildError::ExtensionShape { extension, slot: "explicit", .. } if extension == "RestrictOrigins"
	));
}

#[test]
fn a_build_without_a_call_is_refused() {
	let builder = start_build(runtime(PEOPLE));

	assert!(matches!(assemble(builder, None, None), Err(ExtrinsicBuildError::NoCall)));
}

/// The signature `VerifyMultiSignature` carries in a V5 `extrinsic` whose
/// pipeline puts it right after an empty first extension, as People's does.
fn v5_signature(extrinsic: &[u8], metadata: &Metadata) -> VerifySignatureDetails<SubstrateConfig> {
	let mut input = extrinsic;
	Compact::<u32>::decode(&mut input).unwrap();
	let [0x45, 0, ref values @ ..] = *input else {
		panic!("expected a V5 general extrinsic on pipeline 0");
	};
	let pipeline = metadata.extrinsic_extension_info(Some(0)).unwrap();
	let slot = pipeline
		.extension_ids
		.iter()
		.find(|declared| declared.name == "VerifyMultiSignature");
	let mut cursor = values;
	VerifySignatureDetails::decode_as_type(&mut cursor, slot.unwrap().id, metadata.types()).unwrap()
}

/// A V5 signature covers the extensions after `VerifyMultiSignature`, as the
/// node recomputes them; signing anything else is a `BadProof`.
#[test]
fn a_signed_v5_extrinsic_signs_the_implication_after_verify_multi_signature() {
	let allowance = AllowanceMetadata::decode(PEOPLE).unwrap();
	let call = remark_call(PEOPLE);
	let expected = build_proof_message_after_extension(
		&allowance,
		&call,
		&allowance_state(),
		"VerifyMultiSignature",
	)
	.unwrap();
	let signer = test_signer();

	let mut builder = start_build(runtime(PEOPLE));
	builder
		.with_call_bytes(call)
		.with_signer(&signer)
		.with_nonce(0)
		.with_extension(immortal());
	let built = assemble(builder, Some(0), None).unwrap();
	let metadata = Metadata::decode_from(PEOPLE).unwrap();

	let VerifySignatureDetails::Signed { signature, account } =
		v5_signature(built.extrinsic.bytes(), &metadata)
	else {
		panic!("expected a Signed VerifyMultiSignature");
	};
	assert_eq!(account, signer.account_id());
	assert!(verifies(&signer, &expected, &signature));
}
