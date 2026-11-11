//! Lite-person username registration parameters (signing host, native only).
//!
//! Builds the client-side proofs the identity backend needs to
//! attest a lite username for an account: an sr25519 proof-of-ownership, a
//! bandersnatch ring-VRF member key + plain-VRF proof, and an sr25519
//! consumer-registration signature. The backend submits the on-chain
//! `register_lite_person` extrinsic; the host never signs a chain extrinsic.
//!
//! An individuality runtime that publishes a `SigningContext` constant on the
//! verifying pallet (`PeopleLite` on People, `DotnsGateway` on Asset Hub)
//! checks the account signatures over that context followed by the message.
//! A runtime without the constant checks the message alone.
//! [`signing_context_from_constant`] reads the constant's value. The ring-VRF
//! proof always signs the message alone.
//!
//! Byte layout mirrors signing-bot `src/core/attestation.ts` for backend
//! parity. The registered account is the account whose secret signs here; the
//! paired host resolves the username from the dotNS contracts on Asset Hub
//! (`host_logic::dotns_gateway`), where the backend's `reserve_name` records it.

use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
// Native builds link `verifiable`; the browser core loads it on demand, so a
// browser caller supplies the ring-VRF steps to `build_lite_registration_with`.
#[cfg(not(target_arch = "wasm32"))]
use verifiable::GenerateVerifiable;
#[cfg(not(target_arch = "wasm32"))]
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

#[cfg(not(target_arch = "wasm32"))]
use crate::host_logic::product_account::derive_lite_person_ring_vrf_entropy;
use crate::host_logic::{
	dotns_gateway::build_reservation_message,
	product_account::{
		ProductAccountError, SR25519_SIGNING_CONTEXT, derive_identity_keypair,
		product_public_key_to_address,
	},
	sso::pairing::derive_identity_chat_private_key,
	x25519_seal::X25519PrivateKey,
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
/// The pallet constant an individuality pallet publishes its signing context in.
pub const SIGNING_CONTEXT_CONSTANT: &str = "SigningContext";

/// The signing context a `SigningContext` constant value denotes: its 32
/// bytes. `None`, for an absent constant or one of another width, means the
/// runtime checks the message alone.
pub fn signing_context_from_constant(value: Option<&[u8]>) -> Option<[u8; 32]> {
	value.and_then(|bytes| bytes.try_into().ok())
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

impl LiteRegistration {
	/// Encode the identity backend's registration body, sharing CLI and browser wire bytes.
	pub fn request_body(
		&self,
		username_base: &str,
		reserved_username: Option<&str>,
		signed_at: u64,
	) -> serde_json::Value {
		let hex0x = |bytes: &[u8]| format!("0x{}", hex::encode(bytes));
		let mut dotns = serde_json::json!({
			"signature": hex0x(&self.dotns_signature),
			"signedAt": signed_at,
		});
		if let Some(reserved) = reserved_username {
			dotns["reservedUsername"] = serde_json::json!(reserved);
		}
		serde_json::json!({
			"username": username_base,
			"candidateAccountId": self.candidate_account_id,
			"candidateSignature": hex0x(&self.candidate_signature),
			"ringVrfKey": hex0x(&self.ring_vrf_key),
			"proofOfOwnership": hex0x(&self.proof_of_ownership),
			"identifierKey": hex0x(&self.identifier_key),
			"consumerRegistrationSignature": hex0x(&self.consumer_registration_signature),
			"dotns": dotns,
		})
	}
}

/// Sign the backend auth challenge as the network's UID account, covering the exact body.
pub fn sign_backend_challenge(
	entropy: &[u8],
	network_suffix: &str,
	challenge: &[u8],
	body: &[u8],
) -> Result<([u8; 32], [u8; 64]), ProductAccountError> {
	use sha2::{Digest as _, Sha256};
	let keypair = derive_identity_keypair(entropy, network_suffix)?;
	let client_id = keypair.public.to_bytes();
	let mut hasher = Sha256::new();
	hasher.update(challenge);
	hasher.update(client_id);
	hasher.update(Sha256::digest(body));
	let message: [u8; 32] = hasher.finalize().into();
	let proof = keypair
		.secret
		.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
		.to_bytes();
	Ok((client_id, proof))
}

/// Error while building lite-person registration parameters.
#[derive(Debug, Error)]
pub enum LiteRegistrationError<E: core::fmt::Debug = String> {
	/// RFC-0022 `uid.<suffix>` identity-account derivation failed.
	#[error("uid identity derivation failed: {0:?}")]
	CandidateDerivation(#[from] ProductAccountError),
	/// Ring-VRF proof-of-ownership failed.
	#[error("ring-VRF proof-of-ownership failed: {0:?}")]
	ProofOfOwnership(E),
}

/// Build the lite-person registration parameters for `username_base`
/// (6+ lowercase letters, no digit suffix) against the backend `verifier`.
///
/// `network_suffix` is the dotNS TLD of the network being registered on
/// (`paseo`, `testnet`): the candidate account is `uid.<suffix>` and the member
/// key `peopl.<suffix>`, the same person every other host derives there.
/// The people and Asset Hub signing contexts a registration is signed under,
/// each read from that chain's metadata.
pub struct SigningContexts {
	/// Context for the people chain, if its metadata defines one.
	pub people: Option<[u8; 32]>,
	/// Context for Asset Hub, if its metadata defines one.
	pub asset_hub: Option<[u8; 32]>,
}

/// `reserved_username` optionally queues a base name for a later full-person
/// claim on dotNS. `dotns_signed_at_secs` must be Asset Hub chain time, meaning
/// `Timestamp.Now` in seconds. The local wall clock will not do: the gateway
/// rejects signatures more than 30 seconds in the chain's future.
/// `people_signing_context` and `asset_hub_signing_context` come from
/// [`signing_context_from_constant`] on each chain's metadata.
#[cfg(not(target_arch = "wasm32"))]
pub fn build_lite_registration(
	entropy: &[u8],
	network_suffix: &str,
	verifier_account_id: [u8; 32],
	username_base: &str,
	reserved_username: Option<&str>,
	dotns_signed_at_secs: u64,
	contexts: SigningContexts,
) -> Result<LiteRegistration, LiteRegistrationError<verifiable::Error>> {
	let vrf_secret = BandersnatchVrfVerifiable::new_secret(derive_lite_person_ring_vrf_entropy(
		entropy,
		network_suffix,
	));
	build_lite_registration_with(
		entropy,
		network_suffix,
		verifier_account_id,
		username_base,
		reserved_username,
		dotns_signed_at_secs,
		contexts,
		BandersnatchVrfVerifiable::member_from_secret(&vrf_secret),
		|message| BandersnatchVrfVerifiable::sign(&vrf_secret, message),
	)
}

/// Build registration with the two ring-VRF steps supplied by the caller,
/// for a target that does not link `verifiable`. `ring_vrf_key` is the
/// member key of the lite-person ring-VRF entropy for this entropy and suffix
/// (`product_account::derive_lite_person_ring_vrf_entropy`), and
/// `sign_proof_of_ownership` signs a message with that same key.
#[allow(clippy::too_many_arguments)]
pub fn build_lite_registration_with<E: core::fmt::Debug>(
	entropy: &[u8],
	network_suffix: &str,
	verifier_account_id: [u8; 32],
	username_base: &str,
	reserved_username: Option<&str>,
	dotns_signed_at_secs: u64,
	SigningContexts {
        people: people_signing_context,
        asset_hub: asset_hub_signing_context,
    }: SigningContexts,
	ring_vrf_key: [u8; 32],
	sign_proof_of_ownership: impl FnOnce(&[u8]) -> Result<[u8; 64], E>,
) -> Result<LiteRegistration, LiteRegistrationError<E>> {
	// Registration, local activation, and the SSO responder all use the
	// RFC-0022 `uid.<suffix>` default product account.
	let candidate = derive_identity_keypair(entropy, network_suffix)?;
	let candidate_public_key = candidate.public.to_bytes();

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
	let proof_of_ownership =
		sign_proof_of_ownership(&proof_message).map_err(LiteRegistrationError::ProofOfOwnership)?;

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
	let chat_key = X25519PrivateKey::from(derive_identity_chat_private_key(entropy));
	let public_key = chat_key.public_key().0;
	let mut identifier_key = [0u8; 65];
	identifier_key[0] = IDENTIFIER_KEY_TAG_X25519;
	identifier_key[1..33].copy_from_slice(&public_key);
	identifier_key
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
