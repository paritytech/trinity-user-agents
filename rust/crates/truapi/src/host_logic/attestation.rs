//! Lite-person username registration parameters for signing hosts.
//!
//! Builds the client-side proofs the identity backend needs to
//! attest a lite username for an account: an sr25519 proof-of-ownership, a
//! bandersnatch ring-VRF member key + plain-VRF proof, and an sr25519
//! consumer-registration signature. The backend submits the on-chain
//! `register_lite_person` extrinsic; the host never signs a chain extrinsic.
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

use crate::host_logic::dotns_gateway::build_reservation_message;
#[cfg(not(target_arch = "wasm32"))]
use crate::host_logic::product_account::derive_lite_person_ring_vrf_entropy;
use crate::host_logic::product_account::{
    ProductAccountError, SR25519_SIGNING_CONTEXT, derive_identity_keypair,
    product_public_key_to_address,
};
use crate::host_logic::sso::pairing::{derive_identity_chat_private_key, x25519_public_key};

/// sr25519 proof-of-ownership message prefix (exact bytes; one space).
///
/// Canonical People Lite runtime source:
/// <https://github.com/paritytech/individuality/blob/c3ec60ab934d1a64e4f27d1776a598e839819720/pallets/people-lite/src/lib.rs#L69>
///
/// The pallet verifies `MSG_PREFIX || candidate || ring_vrf_key`.
const REGISTER_PREFIX: &[u8] = b"pop:people-lite:register using";
/// CHAT-RFC-0004 keypair type byte for an X25519 identifier key.
const IDENTIFIER_KEY_TAG_X25519: u8 = 0x00;

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
    /// sr25519 signature over `prefix ‖ candidate_pub ‖ ring_vrf_key`.
    pub candidate_signature: [u8; 64],
    /// Bandersnatch ring-VRF member key.
    pub ring_vrf_key: [u8; 32],
    /// Plain bandersnatch VRF proof over the same proof message.
    pub proof_of_ownership: [u8; 64],
    /// 65-byte CHAT-RFC-0004 identifier key: the `0x00` X25519 type byte, the
    /// 32-byte public key, then 32 zero bytes. It doubles as the dotNS chat key.
    pub identifier_key: [u8; 65],
    /// sr25519 signature over the SCALE consumer-registration tuple.
    pub consumer_registration_signature: [u8; 64],
    /// sr25519 signature over the dotNS gateway reservation message. It
    /// authorizes `pallet_dotns_gateway::reserve_name` on Asset Hub.
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
pub enum LiteRegistrationError {
    /// RFC-0022 `uid.<suffix>` identity-account derivation failed.
    #[error("uid identity derivation failed: {0}")]
    CandidateDerivation(#[from] ProductAccountError),
    /// Ring-VRF proof-of-ownership failed.
    #[error("ring-VRF proof-of-ownership failed: {0}")]
    ProofOfOwnership(String),
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
#[cfg(not(target_arch = "wasm32"))]
pub fn build_lite_registration(
    entropy: &[u8],
    network_suffix: &str,
    verifier_account_id: [u8; 32],
    username_base: &str,
    reserved_username: Option<&str>,
    dotns_signed_at_secs: u64,
) -> Result<LiteRegistration, LiteRegistrationError> {
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
        BandersnatchVrfVerifiable::member_from_secret(&vrf_secret),
        |message| {
            BandersnatchVrfVerifiable::sign(&vrf_secret, message).map_err(|err| format!("{err:?}"))
        },
    )
}

/// [`build_lite_registration`] with the two ring-VRF steps supplied by the
/// caller, for a target that does not link `verifiable`. `ring_vrf_key` is the
/// member key of the lite-person ring-VRF entropy for this entropy and suffix
/// (`product_account::derive_lite_person_ring_vrf_entropy`), and
/// `sign_proof_of_ownership` signs a message with that same key.
#[allow(clippy::too_many_arguments)]
pub fn build_lite_registration_with(
    entropy: &[u8],
    network_suffix: &str,
    verifier_account_id: [u8; 32],
    username_base: &str,
    reserved_username: Option<&str>,
    dotns_signed_at_secs: u64,
    ring_vrf_key: [u8; 32],
    sign_proof_of_ownership: impl FnOnce(&[u8]) -> Result<[u8; 64], String>,
) -> Result<LiteRegistration, LiteRegistrationError> {
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
        .sign_simple(SR25519_SIGNING_CONTEXT, &proof_message, &candidate.public)
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
            &consumer_message,
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
            &reservation_message,
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
        )
        .unwrap();
        assert_eq!(
            reg.candidate_public_key,
            derive_identity_keypair(&ENTROPY, NETWORK_SUFFIX)
                .unwrap()
                .public
                .to_bytes(),
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
        assert!(
            reg.candidate_account_id
                .chars()
                .all(|c| c.is_alphanumeric())
        );

        // candidateSignature verifies over prefix ‖ candidate_pub ‖ ring_vrf_key.
        let mut proof_message = Vec::new();
        proof_message.extend_from_slice(REGISTER_PREFIX);
        proof_message.extend_from_slice(&reg.candidate_public_key);
        proof_message.extend_from_slice(&reg.ring_vrf_key);
        let public = PublicKey::from_bytes(&reg.candidate_public_key).unwrap();
        let sig = Signature::from_bytes(&reg.candidate_signature).unwrap();
        assert!(
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &proof_message, &sig)
                .is_ok(),
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
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &consumer_message, &sig)
                .is_ok(),
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
            public
                .verify_simple(SR25519_SIGNING_CONTEXT, &consumer_message, &sig)
                .is_ok(),
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
        let first =
            build_lite_registration(&ENTROPY, NETWORK_SUFFIX, verifier, "aliceheadless", None, 1)
                .unwrap();
        let again =
            build_lite_registration(&ENTROPY, NETWORK_SUFFIX, verifier, "aliceheadless", None, 1)
                .unwrap();
        assert_eq!(first.candidate_public_key, again.candidate_public_key);
        assert_eq!(first.ring_vrf_key, again.ring_vrf_key);
        assert_eq!(first.candidate_account_id, again.candidate_account_id);
    }
}
