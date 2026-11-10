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
		SigningContexts { people: None, asset_hub: None },
	)
	.unwrap();
	assert_eq!(
		reg.candidate_public_key,
		derive_identity_keypair(&ENTROPY, NETWORK_SUFFIX).unwrap().public.to_bytes(),
		"registration uses the network's peopl.paseo identity account"
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
		&X25519PrivateKey::from(derive_identity_chat_private_key(&ENTROPY))
			.public_key()
			.0,
		"the advertised key must match the chat identity private key this host serves"
	);
	assert_eq!(&reg.identifier_key[33..], &[0u8; 32], "the trailing 32 bytes must be zero-filled");
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
		SigningContexts { people: None, asset_hub: None },
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
		SigningContexts { people: None, asset_hub: None },
	)
	.unwrap();
	let again = build_lite_registration(
		&ENTROPY,
		NETWORK_SUFFIX,
		verifier,
		"aliceheadless",
		None,
		1,
		SigningContexts { people: None, asset_hub: None },
	)
	.unwrap();
	assert_eq!(first.candidate_public_key, again.candidate_public_key);
	assert_eq!(first.ring_vrf_key, again.ring_vrf_key);
	assert_eq!(first.candidate_account_id, again.candidate_account_id);
}

#[test]
fn signatures_in_context_verify_only_with_the_context() {
	// A runtime that publishes its signing context rejects the plain message,
	// so each account signature must cover the context of the chain that
	// checks it.
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
		SigningContexts { people: Some(people), asset_hub: Some(asset_hub) },
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
	assert!(verifies(&reg.consumer_registration_signature, &in_context(people, &consumer_message)));
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
fn signing_context_follows_the_published_constant() {
	assert_eq!(signing_context_from_constant(Some(&[7u8; 32])), Some([7u8; 32]));
	assert_eq!(signing_context_from_constant(None), None);
	// A value of another width is not a context the runtime applies.
	assert_eq!(signing_context_from_constant(Some(&[7u8; 31])), None);
}
