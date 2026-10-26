//! Lite-person username registration parameters (signing host, native only).
//!
//! Builds the client-side proofs the identity backend needs to
//! attest a lite username for an account: an sr25519 proof-of-ownership, a
//! bandersnatch ring-VRF member key + plain-VRF proof, and an sr25519
//! consumer-registration signature. The backend submits the on-chain
//! `register_lite_person` extrinsic; the host never signs a chain extrinsic.
//!
//! Individuality runtimes above spec version 3004000 check the account
//! signatures over the personhood signing context followed by the message,
//! and earlier ones over the message alone. [`signing_context_for`] picks the
//! context from the verifying chain's runtime version. The ring-VRF proof
//! always signs the message alone.
//!
//! Byte layout mirrors signing-bot `src/core/attestation.ts` for backend
//! parity. The registered account is the account whose secret signs here; the
//! paired host resolves the username from the dotNS contracts on Asset Hub
//! (`host_logic::dotns_gateway`), where the backend's `reserve_name` records it.

use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
use verifiable::{
	Error as VerifiableError, GenerateVerifiable, ring::bandersnatch::BandersnatchVrfVerifiable,
};

use crate::host_logic::{
	dotns_gateway::build_reservation_message,
	product_account::{
		ProductAccountError, SR25519_SIGNING_CONTEXT, derive_identity_keypair,
		derive_lite_person_ring_vrf_entropy, personhood_product_id, product_public_key_to_address,
		signing_context,
	},
	sso::pairing::{derive_identity_chat_private_key, x25519_public_key},
};

/// sr25519 proof-of-ownership message prefix (exact bytes; one space).
///
/// Canonical People Lite runtime source:
/// <https://github.com/paritytech/individuality/blob/c3ec60ab934d1a64e4f27d1776a598e839819720/pallets/people-lite/src/lib.rs#L69>
///
/// The pallet verifies `MSG_PREFIX || candidate || ring_vrf_key`.
const REGISTER_PREFIX: &[u8] = b"pop:people-lite:register using";
/// CHAT-RFC-0004 keypair type byte for an X25519 identifier key.
const IDENTIFIER_KEY_TAG_X25519: u8 = 0x00;
/// The last individuality spec version that checks account signatures over
/// the message alone. Delete the plain form once every network runs a later
/// release.
const LAST_PLAIN_SPEC_VERSION: u32 = 3_004_000;
/// The individuality runtimes that verify the registration signatures.
const INDIVIDUALITY_SPEC_NAMES: [&str; 2] = ["next-people-paseo", "next-asset-hub-paseo"];

/// The personhood signing context a runtime with `spec_name` and
/// `spec_version` checks account signatures in, on the network with
/// `network_suffix`. `None` means the message alone.
///
/// Individuality runtimes above spec version 3004000 use the context. Earlier
/// versions and other runtimes do not.
pub fn signing_context_for(
	spec_name: &str,
	spec_version: u32,
	network_suffix: &str,
) -> Option<[u8; 32]> {
	(INDIVIDUALITY_SPEC_NAMES.contains(&spec_name) && spec_version > LAST_PLAIN_SPEC_VERSION)
		.then(|| signing_context(&personhood_product_id(network_suffix)))
}

/// The bytes an account signs for `message`: `context` followed by `message`,
/// or `message` alone.
fn signed_bytes(context: Option<[u8; 32]>, message: &[u8]) -> Vec<u8> {
	match context {
		Some(context) => [context.as_slice(), message].concat(),
		None => message.to_vec(),
	}
}

/// SCALE payload signed for a lite consumer registration.
///
/// This mirrors the People runtime's tuple of account, verifier, identifier
/// key, username base, and optional reserved username.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ConsumerRegistrationSigningPayload {
	account: [u8; 32],
	verifier: [u8; 32],
	identifier_key: [u8; 65],
	username: Vec<u8>,
	reserved_username: Option<Vec<u8>>,
}

/// Client-computed parameters for `POST /usernames`.
pub struct LiteRegistration {
	/// SS58 (prefix 42) of the candidate account.
	pub candidate_account_id: String,
	/// Raw 32-byte candidate public key (the account the username is recorded for).
	pub candidate_public_key: [u8; 32],
	/// sr25519 signature over `prefix ‖ candidate_pub ‖ ring_vrf_key`, after
	/// the People signing context if any.
	pub candidate_signature: [u8; 64],
	/// Bandersnatch ring-VRF member key.
	pub ring_vrf_key: [u8; 32],
	/// Plain bandersnatch VRF proof over the same proof message.
	pub proof_of_ownership: [u8; 64],
	/// 65-byte CHAT-RFC-0004 identifier key: the `0x00` X25519 type byte, the
	/// 32-byte public key, then 32 zero bytes. It doubles as the dotNS chat key.
	pub identifier_key: [u8; 65],
	/// sr25519 signature over the SCALE consumer-registration tuple, after the
	/// People signing context if any.
	pub consumer_registration_signature: [u8; 64],
	/// sr25519 signature over the dotNS gateway reservation message, after the
	/// Asset Hub signing context if any. It authorizes
	/// `pallet_dotns_gateway::reserve_name` on Asset Hub.
	pub dotns_signature: [u8; 64],
}

/// Error while building lite-person registration parameters.
#[derive(Debug, Error, derive_more::From)]
pub enum LiteRegistrationError {
	/// RFC-0022 `uid.<suffix>` identity-account derivation failed.
	#[error("uid identity derivation failed: {0}")]
	CandidateDerivation(#[source] ProductAccountError),
	/// Ring-VRF proof-of-ownership failed.
	#[error("ring-VRF proof-of-ownership failed: {0:?}")]
	ProofOfOwnership(VerifiableError),
}

/// Build the lite-person registration parameters for `username_base`
/// (6+ lowercase letters, no digit suffix) against the backend `verifier`.
///
/// `network_suffix` is the dotNS TLD of the network being registered on
/// (`paseo`, `testnet`): the candidate account is `uid.<suffix>` and the member
/// key `peopl.<suffix>`, the same person every other host derives there.
/// `reserved_username` optionally queues a base name for a later full-person
/// claim on dotNS. `dotns_signed_at_secs` must be Asset Hub chain time, meaning
/// `Timestamp.Now` in seconds. The local wall clock will not do: the gateway
/// rejects signatures more than 30 seconds in the chain's future.
/// `people_signing_context` and `asset_hub_signing_context` come from
/// [`signing_context_for`] on each chain.
#[allow(clippy::too_many_arguments)]
pub fn build_lite_registration(
	entropy: &[u8],
	network_suffix: &str,
	verifier_account_id: [u8; 32],
	username_base: &str,
	reserved_username: Option<&str>,
	dotns_signed_at_secs: u64,
	people_signing_context: Option<[u8; 32]>,
	asset_hub_signing_context: Option<[u8; 32]>,
) -> Result<LiteRegistration, LiteRegistrationError> {
	// Registration, local activation, and the SSO responder all use the
	// RFC-0022 `uid.<suffix>` default product account.
	let candidate = derive_identity_keypair(entropy, network_suffix)?;
	let candidate_public_key = candidate.public.to_bytes();

	let vrf_entropy = derive_lite_person_ring_vrf_entropy(entropy, network_suffix);
	let vrf_secret = BandersnatchVrfVerifiable::new_secret(vrf_entropy);
	let ring_vrf_key = BandersnatchVrfVerifiable::member_from_secret(&vrf_secret);

	let mut proof_message = Vec::with_capacity(REGISTER_PREFIX.len() + 64);
	proof_message.extend_from_slice(REGISTER_PREFIX);
	proof_message.extend_from_slice(&candidate_public_key);
	proof_message.extend_from_slice(&ring_vrf_key);

	let candidate_signature = candidate
		.secret
		.sign_simple(
			SR25519_SIGNING_CONTEXT,
			&signed_bytes(people_signing_context, &proof_message),
			&candidate.public,
		)
		.to_bytes();
	let proof_of_ownership = BandersnatchVrfVerifiable::sign(&vrf_secret, &proof_message)?;

	let identifier_key = derive_identifier_key(entropy);

	let consumer_message = ConsumerRegistrationSigningPayload {
		account: candidate_public_key,
		verifier: verifier_account_id,
		identifier_key,
		username: username_base.as_bytes().to_vec(),
		reserved_username: reserved_username.map(|name| name.as_bytes().to_vec()),
	}
	.encode();
	let consumer_registration_signature = candidate
		.secret
		.sign_simple(
			SR25519_SIGNING_CONTEXT,
			&signed_bytes(people_signing_context, &consumer_message),
			&candidate.public,
		)
		.to_bytes();

	let reservation_message = build_reservation_message(
		&candidate_public_key,
		&verifier_account_id,
		username_base.as_bytes(),
		&identifier_key,
		reserved_username.map(str::as_bytes),
		dotns_signed_at_secs,
	);
	let dotns_signature = candidate
		.secret
		.sign_simple(
			SR25519_SIGNING_CONTEXT,
			&signed_bytes(asset_hub_signing_context, &reservation_message),
			&candidate.public,
		)
		.to_bytes();

	Ok(LiteRegistration {
		candidate_account_id: product_public_key_to_address(candidate_public_key),
		candidate_public_key,
		candidate_signature,
		ring_vrf_key,
		proof_of_ownership,
		identifier_key,
		consumer_registration_signature,
		dotns_signature,
	})
}

/// The identity's chat public key in its CHAT-RFC-0004 envelope.
///
/// The key is the public half of the X25519 identity chat key, so what is
/// advertised on chain is the counterpart of the private key this host serves
/// to a paired chat client. The 65-byte width predates X25519 and stayed when
/// the curve changed; readers ignore the padding rather than validate it.
fn derive_identifier_key(entropy: &[u8]) -> [u8; 65] {
	let public_key = x25519_public_key(derive_identity_chat_private_key(entropy));
	let mut identifier_key = [0u8; 65];
	identifier_key[0] = IDENTIFIER_KEY_TAG_X25519;
	identifier_key[1..33].copy_from_slice(&public_key);
	identifier_key
}

#[cfg(test)]
mod tests {
	use super::*;
	use schnorrkel::{PublicKey, Signature};

	const ENTROPY: [u8; 16] = [0xAB; 16];
	const NETWORK_SUFFIX: &str = "paseo";

	#[test]
	fn registration_params_have_expected_shapes_and_verify() {
		let verifier = [0x11u8; 32];
		let reg = build_lite_registration(
			&ENTROPY,
			NETWORK_SUFFIX,
			verifier,
			"headlesstester",
			None,
			1_749_573_123,
			None,
			None,
		)
		.unwrap();
		assert_eq!(
			reg.candidate_public_key,
			derive_identity_keypair(&ENTROPY, NETWORK_SUFFIX).unwrap().public.to_bytes(),
			"registration uses the network's uid.paseo identity account"
		);
		let lite_entropy = derive_lite_person_ring_vrf_entropy(&ENTROPY, NETWORK_SUFFIX);
		assert_eq!(
			reg.ring_vrf_key,
			BandersnatchVrfVerifiable::member_from_secret(&BandersnatchVrfVerifiable::new_secret(
				lite_entropy
			)),
			"registration uses the network's peopl.paseo index-1 member"
		);
		assert_ne!(
			reg.ring_vrf_key,
			BandersnatchVrfVerifiable::member_from_secret(&BandersnatchVrfVerifiable::new_secret(
				derive_lite_person_ring_vrf_entropy(&ENTROPY, "dot")
			)),
			"a person registered on paseo-next-v2 is not the seed's .dot person"
		);

		// CHAT-RFC-0004: `0x00` tag, 32-byte X25519 key, 32 zero bytes.
		assert_eq!(reg.identifier_key[0], 0x00, "X25519 keypair type byte");
		assert_eq!(
			&reg.identifier_key[1..33],
			&x25519_public_key(derive_identity_chat_private_key(&ENTROPY)),
			"the advertised key must match the chat identity private key this host serves"
		);
		assert_eq!(
			&reg.identifier_key[33..],
			&[0u8; 32],
			"the trailing 32 bytes must be zero-filled"
		);
		assert!(reg.candidate_account_id.chars().all(|c| c.is_alphanumeric()));

		// candidateSignature verifies over prefix ‖ candidate_pub ‖ ring_vrf_key.
		let mut proof_message = Vec::new();
		proof_message.extend_from_slice(REGISTER_PREFIX);
		proof_message.extend_from_slice(&reg.candidate_public_key);
		proof_message.extend_from_slice(&reg.ring_vrf_key);
		let public = PublicKey::from_bytes(&reg.candidate_public_key).unwrap();
		let sig = Signature::from_bytes(&reg.candidate_signature).unwrap();
		assert!(
			public.verify_simple(SR25519_SIGNING_CONTEXT, &proof_message, &sig).is_ok(),
			"candidate signature verifies"
		);

		// proofOfOwnership verifies as a plain VRF signature for the member key.
		assert!(
			BandersnatchVrfVerifiable::verify_signature(
				&reg.proof_of_ownership,
				&proof_message,
				&reg.ring_vrf_key
			),
			"ring-VRF proof-of-ownership validates against the member key"
		);

		// Verify against the runtime tuple independently of the production
		// payload struct so field-order or optional-field regressions fail.
		let consumer_message = (
			reg.candidate_public_key,
			verifier,
			reg.identifier_key,
			b"headlesstester".as_slice(),
			None::<Vec<u8>>,
		)
			.encode();
		let sig = Signature::from_bytes(&reg.consumer_registration_signature).unwrap();
		assert!(
			public.verify_simple(SR25519_SIGNING_CONTEXT, &consumer_message, &sig).is_ok(),
			"consumer registration signature verifies against the runtime tuple"
		);

		// dotnsSignature verifies over the gateway reservation message. The
		// identifier key doubles as the chat key.
		let reservation_message = build_reservation_message(
			&reg.candidate_public_key,
			&verifier,
			b"headlesstester",
			&reg.identifier_key,
			None,
			1_749_573_123,
		);
		let sig = Signature::from_bytes(&reg.dotns_signature).unwrap();
		assert!(
			public
				.verify_simple(SR25519_SIGNING_CONTEXT, &reservation_message, &sig)
				.is_ok(),
			"dotns reservation signature verifies against the gateway message"
		);
	}

	#[test]
	fn reserved_username_threads_into_both_signed_payloads() {
		let verifier = [0x33u8; 32];
		let reg = build_lite_registration(
			&ENTROPY,
			NETWORK_SUFFIX,
			verifier,
			"headlesstester",
			Some("reservedbase"),
			77,
			None,
			None,
		)
		.unwrap();
		let public = PublicKey::from_bytes(&reg.candidate_public_key).unwrap();

		let consumer_message = (
			reg.candidate_public_key,
			verifier,
			reg.identifier_key,
			b"headlesstester".as_slice(),
			Some(b"reservedbase".to_vec()),
		)
			.encode();
		let sig = Signature::from_bytes(&reg.consumer_registration_signature).unwrap();
		assert!(
			public.verify_simple(SR25519_SIGNING_CONTEXT, &consumer_message, &sig).is_ok(),
			"consumer registration signature commits to the reserved username"
		);

		let reservation_message = build_reservation_message(
			&reg.candidate_public_key,
			&verifier,
			b"headlesstester",
			&reg.identifier_key,
			Some(b"reservedbase"),
			77,
		);
		let sig = Signature::from_bytes(&reg.dotns_signature).unwrap();
		assert!(
			public
				.verify_simple(SR25519_SIGNING_CONTEXT, &reservation_message, &sig)
				.is_ok(),
			"dotns signature commits to the reserved username and signed_at"
		);
	}

	#[test]
	fn consumer_registration_payload_matches_runtime_tuple_codec() {
		let payload = ConsumerRegistrationSigningPayload {
			account: [0x11; 32],
			verifier: [0x22; 32],
			identifier_key: [0x04; 65],
			username: b"headlesstester".to_vec(),
			reserved_username: None,
		};
		let encoded = payload.encode();
		let runtime_tuple = (
			payload.account,
			payload.verifier,
			payload.identifier_key,
			payload.username.as_slice(),
			payload.reserved_username.as_ref(),
		)
			.encode();

		assert_eq!(encoded, runtime_tuple);
		assert_eq!(
			ConsumerRegistrationSigningPayload::decode(&mut encoded.as_slice()).unwrap(),
			payload
		);
	}

	#[test]
	fn registration_is_deterministic_per_entropy_and_username() {
		let verifier = [0x22u8; 32];
		let first = build_lite_registration(
			&ENTROPY,
			NETWORK_SUFFIX,
			verifier,
			"aliceheadless",
			None,
			1,
			None,
			None,
		)
		.unwrap();
		let again = build_lite_registration(
			&ENTROPY,
			NETWORK_SUFFIX,
			verifier,
			"aliceheadless",
			None,
			1,
			None,
			None,
		)
		.unwrap();
		assert_eq!(first.candidate_public_key, again.candidate_public_key);
		assert_eq!(first.ring_vrf_key, again.ring_vrf_key);
		assert_eq!(first.candidate_account_id, again.candidate_account_id);
	}

	#[test]
	fn signatures_in_context_verify_only_with_the_context() {
		// Above spec 3004000 the runtimes reject the plain message, so each
		// account signature must cover the context of the chain that checks it.
		let verifier = [0x44u8; 32];
		let people = [0x01u8; 32];
		let asset_hub = [0x02u8; 32];
		let reg = build_lite_registration(
			&ENTROPY,
			NETWORK_SUFFIX,
			verifier,
			"headlesstester",
			None,
			9,
			Some(people),
			Some(asset_hub),
		)
		.unwrap();
		let public = PublicKey::from_bytes(&reg.candidate_public_key).unwrap();
		let verifies = |signature: &[u8; 64], message: &[u8]| {
			public
				.verify_simple(
					SR25519_SIGNING_CONTEXT,
					message,
					&Signature::from_bytes(signature).unwrap(),
				)
				.is_ok()
		};
		let in_context = |context: [u8; 32], message: &[u8]| [context.as_slice(), message].concat();

		let mut proof_message = Vec::new();
		proof_message.extend_from_slice(REGISTER_PREFIX);
		proof_message.extend_from_slice(&reg.candidate_public_key);
		proof_message.extend_from_slice(&reg.ring_vrf_key);
		let consumer_message = (
			reg.candidate_public_key,
			verifier,
			reg.identifier_key,
			b"headlesstester".as_slice(),
			None::<Vec<u8>>,
		)
			.encode();
		let reservation_message = build_reservation_message(
			&reg.candidate_public_key,
			&verifier,
			b"headlesstester",
			&reg.identifier_key,
			None,
			9,
		);

		assert!(verifies(&reg.candidate_signature, &in_context(people, &proof_message)));
		assert!(!verifies(&reg.candidate_signature, &proof_message));
		assert!(verifies(
			&reg.consumer_registration_signature,
			&in_context(people, &consumer_message)
		));
		assert!(!verifies(&reg.consumer_registration_signature, &consumer_message));
		assert!(verifies(&reg.dotns_signature, &in_context(asset_hub, &reservation_message)));
		assert!(!verifies(&reg.dotns_signature, &in_context(people, &reservation_message)));
		// The ring-VRF proof keeps signing the message alone.
		assert!(BandersnatchVrfVerifiable::verify_signature(
			&reg.proof_of_ownership,
			&proof_message,
			&reg.ring_vrf_key
		));
	}

	#[test]
	fn signing_context_follows_the_individuality_spec_version() {
		let context = signing_context(&personhood_product_id("paseo"));

		assert_eq!(signing_context_for("next-people-paseo", 3_004_000, "paseo"), None);
		assert_eq!(signing_context_for("next-people-paseo", 3_004_001, "paseo"), Some(context));
		assert_eq!(signing_context_for("next-asset-hub-paseo", 3_004_001, "paseo"), Some(context));
		// Other runtimes keep the plain message whatever their version.
		assert_eq!(signing_context_for("people-paseo", 9_000_000, "paseo"), None);
	}
}
