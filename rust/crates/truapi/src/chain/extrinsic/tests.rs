//! Builder tests against the fixture runtimes. Each byte-level test compares
//! with an independent builder: the statement allowance assembler, its
//! implication, or subxt's own V4 transaction.

use std::sync::{Arc, Mutex};

use futures::executor::block_on;

use parity_scale_codec::{Compact, Decode, Encode};
use sp_crypto_hashing::blake2_256;

use subxt::{
	Metadata,
	config::{
		DefaultExtrinsicParamsBuilder, substrate::SubstrateConfig,
		transaction_extensions::VerifySignatureDetails,
	},
	dynamic::{Value, tx},
	ext::{
		frame_decode::extrinsics::ExtrinsicTypeInfo,
		scale_decode::DecodeAsType,
		scale_encode::{EncodeAsType, Error as EncodeError, TypeResolver},
	},
	tx::{Payload, Signer},
	utils::{AccountId32, H256, MultiSignature},
};

use super::{
	ExtensionError, ExtrinsicBuildError, ExtrinsicBuilder, ExtrinsicVersion, InheritedImplication,
	TransactionExtension, builder::RuntimeAtBirth, signature::VerifySignatureExtension,
	version::choose_extrinsic_version, well_known::default_value_for_type,
};
use crate::{
	chain::{HashAndNumber, MortalExtrinsic, extrinsic::tests::offline_source::OfflineSource},
	host_internal::extrinsic::{
		Sr25519Signer,
		tests::{bulletin_chain_state, split_v4},
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

mod offline_source;

fn runtime(metadata_bytes: &[u8]) -> RuntimeAtBirth {
	RuntimeAtBirth {
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

fn remark() -> impl Payload + Send {
	tx("System", "remark", vec![Value::from_bytes([1, 2, 3])])
}

/// A source for a chain running the People runtime.
fn people() -> OfflineSource {
	OfflineSource::new(H256(GENESIS), runtime(PEOPLE))
}

/// A source for the Bulletin fixture chain.
fn bulletin() -> OfflineSource {
	OfflineSource::new(H256(bulletin_chain_state().genesis_hash), bulletin_runtime())
}

fn build_offline(builder: ExtrinsicBuilder<'_>) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	block_on(builder.build())
}

/// A value that is already encoded, written as it is whatever the type.
#[derive(Clone, Debug)]
struct PreencodedValue(Vec<u8>);

impl EncodeAsType for PreencodedValue {
	fn encode_as_type_to<R: TypeResolver>(
		&self,
		_type_id: R::TypeId,
		_types: &R,
		out: &mut Vec<u8>,
	) -> Result<(), EncodeError> {
		out.extend_from_slice(&self.0);
		Ok(())
	}
}

/// An extension with fixed, already-encoded values, for byte-level tests
/// against reference extrinsics.
struct FixedBytesExtension {
	name: &'static str,
	explicit: Vec<u8>,
	implicit: Vec<u8>,
}

impl FixedBytesExtension {
	fn explicit_only(name: &'static str, explicit: Vec<u8>) -> Self {
		Self { name, explicit, implicit: Vec::new() }
	}
}

impl TransactionExtension for FixedBytesExtension {
	type Implicit = PreencodedValue;
	type Explicit = PreencodedValue;

	fn name(&self) -> &str {
		self.name
	}

	fn implicit(&self) -> PreencodedValue {
		PreencodedValue(self.implicit.clone())
	}

	fn explicit(
		&self,
		_implication: &InheritedImplication,
	) -> Result<PreencodedValue, ExtensionError> {
		Ok(PreencodedValue(self.explicit.clone()))
	}
}

/// `CheckMortality` as the allowance assembler writes it: immortal, with the
/// genesis hash as its implicit.
fn immortal() -> FixedBytesExtension {
	FixedBytesExtension { name: "CheckMortality", explicit: vec![0x00], implicit: GENESIS.to_vec() }
}

/// `Some(ClaimLongTermStorage(proof, 3, 9, LitePeople))`, the People
/// `AsResources` value, whose variant indices are 3 and 1.
fn long_term_storage_claim_value() -> Vec<u8> {
	[vec![0x01, 0x03], vec![0xEEu8; 785].encode(), 3u32.encode(), 9u32.encode(), vec![0x01]]
		.concat()
}

/// Records the implication it was given, then answers with fixed bytes.
struct RecordingExtension {
	name: &'static str,
	explicit: Vec<u8>,
	seen: Arc<Mutex<Option<InheritedImplication>>>,
}

impl TransactionExtension for RecordingExtension {
	type Implicit = ();
	type Explicit = PreencodedValue;

	fn name(&self) -> &str {
		self.name
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<PreencodedValue, ExtensionError> {
		*self.seen.lock().unwrap() = Some(implication.clone());
		Ok(PreencodedValue(self.explicit.clone()))
	}
}

/// Blake2-256 of the 822-byte authorized extrinsic the People runtime accepts
/// for this call and `AsResources` value with an immortal era.
const AUTHORIZED_EXTRINSIC_HASH: &str =
	"98c8c0ea429d862856c9de737268cef03340fb77c3a4f0c39fb56e7125e494e2";

/// The proof-authorized layout the People runtime accepts: preamble,
/// extension order, the authorizer's value in its place, then the call.
#[test]
fn an_authorized_extrinsic_has_the_reference_bytes() {
	let extra = long_term_storage_claim_value();

	let source = people();
	let builder = source
		.start_build()
		.with_call(remark())
		.with_extension(FixedBytesExtension::explicit_only("AsResources", extra))
		.with_extension(immortal())
		.with_extrinsic_version(ExtrinsicVersion::V5 { extension_version: 0 });
	let built = build_offline(builder).unwrap();

	assert_eq!(hex::encode(blake2_256(built.extrinsic.bytes())), AUTHORIZED_EXTRINSIC_HASH);
}

/// A ring proof covers exactly the extensions after its authorizer; one more
/// or one fewer and the node rejects the proof.
///
/// The hash is the reference implication for this call after `AsResources`.
#[test]
fn an_authorizer_sees_the_implication_of_the_extensions_after_it() {
	let expected = "d0b6630d1e1764486b2080946470856f3b00d9408d71d23ab48c40d3eaf2f043";
	let seen = Arc::new(Mutex::new(None));

	let source = people();
	let builder = source
		.start_build()
		.with_call(remark())
		.with_extension(RecordingExtension {
			name: "AsResources",
			explicit: vec![0x00],
			seen: seen.clone(),
		})
		.with_extension(immortal())
		.with_extrinsic_version(ExtrinsicVersion::V5 { extension_version: 0 });
	build_offline(builder).unwrap();

	let implication_hash = seen.lock().unwrap().as_ref().map(InheritedImplication::hash);
	assert_eq!(implication_hash.map(hex::encode).as_deref(), Some(expected));
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
		birth: BIRTH,
		spec_version: state.spec_version,
		transaction_version: state.transaction_version,
		metadata: state.metadata,
	}
}

fn build_bulletin_remark(
	signer: &Sr25519Signer,
	nonce_override: Option<u64>,
	account_nonce: u64,
) -> Vec<u8> {
	let source = bulletin().with_account_nonce(account_nonce);
	let mut builder = source.start_build().with_call(remark()).with_signer(signer);
	if let Some(nonce) = nonce_override {
		builder = builder.with_nonce(nonce);
	}
	build_offline(builder).unwrap().extrinsic.bytes().to_vec()
}

/// Bulletin builds through subxt today; the builder must produce the same
/// V4 extrinsic and a signature over subxt's own payload.
#[test]
fn a_signed_v4_extrinsic_matches_subxt() {
	let signer = test_signer();
	let built = build_bulletin_remark(&signer, None, 7);
	let (account, signature, _) = split_v4(&built);
	let signature = MultiSignature::Sr25519(signature);

	let (expected, payload) = subxt_bulletin_v4(7, &AccountId32(account), &signature);

	assert_eq!(built, expected);
	assert!(verifies(&signer, &payload, &signature));
}

/// The signature is an ordinary extension wherever it was added: V4 finds it
/// in the list and signs the preamble with it.
#[test]
fn a_v4_extrinsic_signs_with_a_signature_added_as_an_extension() {
	let signer = test_signer();
	let source = bulletin().with_account_nonce(7);
	let builder = source
		.start_build()
		.with_call(remark())
		.with_extension(VerifySignatureExtension::signed(&signer));
	let built = build_offline(builder).unwrap().extrinsic.bytes().to_vec();
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
	let built = build_bulletin_remark(&signer, Some(5), 7);
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

/// How many times building `builder` reads the signer's nonce from `source`.
fn account_nonce_reads(source: &OfflineSource, builder: ExtrinsicBuilder<'_>) -> usize {
	let _ = build_offline(builder);
	source.nonce_reads()
}

/// The pool nonce is read only when the build signs and the caller set none.
#[test]
fn only_a_signed_build_without_a_nonce_reads_the_account_nonce() {
	let signer = test_signer();
	let (signed, pinned, unsigned) = (bulletin(), bulletin(), bulletin());

	let signed_builder = signed.start_build().with_call(remark()).with_signer(&signer);
	let signed_reads = account_nonce_reads(&signed, signed_builder);
	let pinned_builder =
		pinned.start_build().with_call(remark()).with_signer(&signer).with_nonce(1);
	let pinned_reads = account_nonce_reads(&pinned, pinned_builder);
	let unsigned_reads = account_nonce_reads(&unsigned, unsigned.start_build().with_call(remark()));

	assert_eq!(signed_reads, 1);
	assert_eq!(pinned_reads, 0);
	assert_eq!(unsigned_reads, 0);
}

fn default_version(
	host_extension_version: Option<u8>,
	signed: bool,
	metadata_bytes: &[u8],
) -> ExtrinsicVersion {
	let source = OfflineSource::new(H256(GENESIS), runtime(metadata_bytes))
		.with_host_extension_version(host_extension_version);
	let metadata = Metadata::decode_from(metadata_bytes).unwrap();
	let version = choose_extrinsic_version(None, &source, H256(GENESIS), signed, &metadata);
	block_on(version).unwrap()
}

/// The host's map decides the default exactly as the native hosts do.
#[test]
fn the_host_map_decides_the_default_version() {
	let v5_zero = ExtrinsicVersion::V5 { extension_version: 0 };

	assert_eq!(default_version(None, true, PEOPLE), ExtrinsicVersion::V4);
	assert_eq!(default_version(Some(0), true, PEOPLE), v5_zero);
	// Asset Hub's pipeline 0 has no `VerifyMultiSignature`.
	assert_eq!(default_version(Some(0), true, ASSET_HUB), ExtrinsicVersion::V4);
	assert_eq!(default_version(Some(0), false, ASSET_HUB), v5_zero);
}

/// A pinned version never asks the host, so a host that cannot answer does
/// not fail a build that does not need its answer.
#[test]
fn a_pinned_version_does_not_read_the_host() {
	let source = people().with_host_extension_version(Some(0));
	let metadata = Metadata::decode_from(PEOPLE).unwrap();
	let pinned = Some(ExtrinsicVersion::V4);

	let version = choose_extrinsic_version(pinned, &source, H256(GENESIS), true, &metadata);

	assert_eq!(block_on(version).unwrap(), ExtrinsicVersion::V4);
	assert_eq!(source.host_reads(), 0);
}

fn build_error(
	metadata_bytes: &[u8],
	signer: Option<&Sr25519Signer>,
	setup: impl FnOnce(ExtrinsicBuilder<'_>) -> ExtrinsicBuilder<'_>,
) -> ExtrinsicBuildError {
	let source = OfflineSource::new(H256(GENESIS), runtime(metadata_bytes));
	let mut builder = source.start_build().with_call(remark());
	if let Some(signer) = signer {
		builder = builder.with_signer(signer);
	}
	build_offline(setup(builder)).unwrap_err()
}

#[test]
fn a_v4_extrinsic_without_a_signer_is_refused() {
	let error = build_error(PEOPLE, None, |builder| builder);

	assert!(matches!(error, ExtrinsicBuildError::UnsignedV4));
}

/// The signature is an ordinary extension, so a pipeline that does not list it
/// refuses it by name rather than dropping it in silence.
#[test]
fn a_signer_on_a_pipeline_without_verify_multi_signature_is_refused() {
	let signer = test_signer();
	let v5_zero = ExtrinsicVersion::V5 { extension_version: 0 };
	let error =
		build_error(ASSET_HUB, Some(&signer), |builder| builder.with_extrinsic_version(v5_zero));

	assert!(matches!(
		error,
		ExtrinsicBuildError::UndeclaredExtension { extension, extension_version: 0, .. }
			if extension == "VerifyMultiSignature"
	));
}

#[test]
fn an_extension_the_pipeline_does_not_list_is_refused() {
	let pgas_v0 = ExtrinsicVersion::V5 { extension_version: 0 };
	let error = build_error(ASSET_HUB, None, |builder| {
		builder
			.with_extension(FixedBytesExtension::explicit_only("AsPgas", vec![0x00]))
			.with_extrinsic_version(pgas_v0)
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
			.with_extension(FixedBytesExtension::explicit_only("RestrictOrigins", vec![0x01]))
			.with_extension(FixedBytesExtension::explicit_only("RestrictOrigins", vec![0x00]))
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
		builder
			.with_extension(FixedBytesExtension::explicit_only("RestrictOrigins", vec![0x00, 0x00]))
	});

	assert!(matches!(
		error,
		ExtrinsicBuildError::ExtensionShape { extension, value: "explicit", .. } if extension == "RestrictOrigins"
	));
}

#[test]
fn a_build_without_a_call_is_refused() {
	let source = people();

	assert!(matches!(build_offline(source.start_build()), Err(ExtrinsicBuildError::NoCall)));
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
	let declared = pipeline
		.extension_ids
		.iter()
		.find(|declared| declared.name == "VerifyMultiSignature");
	let mut cursor = values;
	VerifySignatureDetails::decode_as_type(&mut cursor, declared.unwrap().id, metadata.types())
		.unwrap()
}

/// A V5 signature covers the extensions after `VerifyMultiSignature`, as the
/// node recomputes them; signing anything else is a `BadProof`. The hash is
/// the reference implication after `VerifyMultiSignature` for this call.
#[test]
fn a_signed_v5_extrinsic_signs_the_implication_after_verify_multi_signature() {
	let expected: [u8; 32] =
		hex::decode("3cc30056f7aa6877d1486175e40187e291176e2034f267db36efcb5e95f63c3d")
			.unwrap()
			.try_into()
			.unwrap();
	let signer = test_signer();

	let source = people().with_host_extension_version(Some(0));
	let builder = source
		.start_build()
		.with_call(remark())
		.with_signer(&signer)
		.with_nonce(0)
		.with_extension(immortal());
	let built = build_offline(builder).unwrap();
	let metadata = Metadata::decode_from(PEOPLE).unwrap();

	let VerifySignatureDetails::Signed { signature, account } =
		v5_signature(built.extrinsic.bytes(), &metadata)
	else {
		panic!("expected a Signed VerifyMultiSignature");
	};
	assert_eq!(account, signer.account_id());
	assert!(verifies(&signer, &expected, &signature));
}

/// `CheckGenesis` signs the genesis the build was given, so a node reporting
/// another chain's genesis cannot make the extrinsic valid there.
#[test]
fn the_implication_carries_the_given_genesis() {
	let configured = H256([0x5c; 32]);
	let seen = Arc::new(Mutex::new(None));
	let source = OfflineSource::new(configured, runtime(PEOPLE));
	let builder = source
		.start_build()
		.with_call(remark())
		.with_extension(RecordingExtension {
			name: "AsResources",
			explicit: vec![0x00],
			seen: seen.clone(),
		})
		.with_extrinsic_version(ExtrinsicVersion::V5 { extension_version: 0 });
	build_offline(builder).unwrap();

	let implication = seen.lock().unwrap().clone().unwrap();
	let carries_configured = implication
		.encoded()
		.windows(32)
		.any(|window| window == configured.0.as_slice());
	assert!(carries_configured);
}

fn asset_hub_value_type(name: &str, implicit: bool) -> u32 {
	let metadata = Metadata::decode_from(ASSET_HUB).unwrap();
	let pipeline = metadata.extrinsic_extension_info(Some(1)).unwrap();
	let declared = pipeline.extension_ids.iter().find(|declared| declared.name == name).unwrap();
	if implicit { declared.implicit_id } else { declared.id }
}

/// An unset extension is `None` when its value is an `Option`, nothing when
/// its type is empty, and otherwise has no default, so the build names it.
#[test]
fn only_empty_and_option_extensions_have_an_unset_default() {
	let metadata = Metadata::decode_from(ASSET_HUB).unwrap();
	let types = metadata.types();
	let as_pgas = asset_hub_value_type("AsPgas", false);
	let check_weight = asset_hub_value_type("CheckWeight", false);
	let restrict_origins = asset_hub_value_type("RestrictOrigins", false);

	assert_eq!(default_value_for_type(as_pgas, types), Some(vec![0x00]));
	assert_eq!(default_value_for_type(check_weight, types), Some(Vec::new()));
	assert_eq!(default_value_for_type(restrict_origins, types), None);
}

/// Bulletin's runtime accepts only V4, so pinning V5 there fails by name
/// rather than producing bytes the node rejects.
#[test]
fn a_version_the_runtime_does_not_accept_is_refused() {
	let signer = test_signer();
	let v5 = ExtrinsicVersion::V5 { extension_version: 0 };
	let source = bulletin();
	let builder = source
		.start_build()
		.with_call(remark())
		.with_signer(&signer)
		.with_nonce(0)
		.with_extrinsic_version(v5);

	let error = build_offline(builder).unwrap_err();

	assert!(
		matches!(error, ExtrinsicBuildError::UnsupportedVersion { supported, .. } if supported == [4])
	);
}
