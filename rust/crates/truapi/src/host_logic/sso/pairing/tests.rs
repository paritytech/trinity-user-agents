use x25519_dalek::{PublicKey, StaticSecret};

use super::*;
use crate::host_logic::statement_store::statement_public_key_from_secret;

const SS_PUBLIC: [u8; 32] = [
	0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
	0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

fn x25519_keypair(seed: u8) -> ([u8; 32], [u8; 32]) {
	let secret_bytes = [seed; 32];
	let secret = StaticSecret::from(secret_bytes);
	(secret_bytes, PublicKey::from(&secret).to_bytes())
}

fn runtime_config() -> PairingHostConfig {
	PairingHostConfig::new(
		HostInfo {
			name: "Polkadot Web".to_string(),
			icon: Some("https://example.invalid/dotli.png".to_string()),
			version: Some("1.2.3".to_string()),
			platform: truapi::latest::HostPlatform::Web,
		},
		PlatformInfo { kind: Some("Firefox".to_string()), version: Some("192.32".to_string()) },
		[0; 32],
		[0xbb; 32],
		[0xcc; 32],
		"polkadotapp".to_string(),
	)
	.expect("test runtime config is valid")
}

fn response(public_key: [u8; 32]) -> v2::EncryptedResponse {
	v2::EncryptedResponse::Success(Box::new(v2::Success {
		identity_account_id: [8; 32],
		root_account_id: [7; 32],
		identity_chat_private_key: [6; 32],
		sso_enc_pub_key: public_key,
		device_enc_pub_key: public_key,
		root_entropy_source: [5; 32],
	}))
}

fn bootstrap(secret: [u8; 32], public: [u8; 32]) -> PairingBootstrap {
	PairingBootstrap {
		deeplink: "polkadotapp://pair?handshake=00".to_string(),
		topic: [0x11; 32],
		statement_store_public_key: [0x22; 32],
		statement_store_secret: [0x33; 64],
		encryption_public_key: public,
		encryption_secret_key: secret,
	}
}

#[test]
fn pairing_deeplink_carries_raw_x25519_key() {
	let (_, public) = x25519_keypair(1);
	let deeplink = build_pairing_deeplink("polkadotapp", SS_PUBLIC, public, &runtime_config());
	assert!(deeplink.starts_with("polkadotapp://pair?handshake=01"));

	let VersionedHandshakeProposal::V2(proposal) = decode_pairing_deeplink(&deeplink).unwrap();
	assert_eq!(proposal.device.statement_account_id, SS_PUBLIC);
	assert_eq!(proposal.device.encryption_public_key, public);
	assert_eq!(proposal.device.encryption_public_key.len(), 32);
}

#[test]
fn established_session_carries_the_account_the_deeplink_advertises() {
	let identity = generate_pairing_device_identity().unwrap();
	let bootstrap = create_pairing_bootstrap_from_identity(&runtime_config(), identity).unwrap();
	let VersionedHandshakeProposal::V2(proposal) =
		decode_pairing_deeplink(&bootstrap.deeplink).unwrap();

	let session = establish_sso_session_info(&bootstrap, [0x77; 32], x25519_keypair(3).1).unwrap();

	// The wallet registers the statement-store allowance for the account in
	// the proposal, so the session has to sign with that same account.
	assert_eq!(session.ss_public_key, proposal.device.statement_account_id);
	assert_eq!(
		statement_public_key_from_secret(session.ss_secret).unwrap(),
		proposal.device.statement_account_id
	);
}

#[test]
fn deeplink_decoder_accepts_bare_hex_and_rejects_trailing_bytes() {
	let (_, public) = x25519_keypair(1);
	let deeplink = build_pairing_deeplink("polkadotapp", SS_PUBLIC, public, &runtime_config());
	let hex_payload = deeplink.split("handshake=").nth(1).unwrap();
	assert_eq!(
		decode_pairing_deeplink(hex_payload).unwrap(),
		decode_pairing_deeplink(&deeplink).unwrap()
	);
	assert!(matches!(
		decode_pairing_deeplink(&format!("{deeplink}00")).unwrap_err(),
		PairingDeeplinkDecodeError::TrailingBytes
	));
}

#[test]
fn app_handshake_codec_uses_32_byte_ephemeral_key() {
	let (_, public) = x25519_keypair(2);
	let answer =
		VersionedHandshakeResponse::V2 { encrypted_message: vec![0xde, 0xad], public_key: public };
	assert_eq!(decode_app_handshake_data(&answer.encode()).unwrap(), answer);
}

#[test]
fn handshake_x25519_chacha_round_trip_and_authentication() {
	let (host_secret, host_public) = x25519_keypair(1);
	let sensitive = response(x25519_keypair(2).1);
	let VersionedHandshakeResponse::V2 { mut encrypted_message, public_key } =
		encrypt_v2_handshake_response(host_public, &sensitive).unwrap();

	assert_eq!(
		decrypt_v2_handshake_response(host_secret, public_key, &encrypted_message).unwrap(),
		sensitive
	);
	let last = encrypted_message.len() - 1;
	encrypted_message[last] ^= 1;
	assert!(decrypt_v2_handshake_response(host_secret, public_key, &encrypted_message).is_err());
}

#[test]
fn rejects_short_handshake_ciphertext_and_small_order_key() {
	assert!(
		decrypt_v2_handshake_response([1; 32], x25519_keypair(2).1, &[0; AEAD_NONCE_LEN + 15],)
			.unwrap_err()
			.contains("too short")
	);
	assert_eq!(
		shared_secret([1; 32], [0; 32]).err().unwrap(),
		format!("invalid X25519 public key {}: a low-order point", hex::encode([0; 32]))
	);
}

#[test]
fn rfc0022_entropy_domains_produce_stable_distinct_keys() {
	let sso = derive_x25519_keypair_from_entropy(&[0xAB; 16], b"sso");
	let chat = derive_x25519_keypair_from_entropy(&[0xAB; 16], b"chat");
	assert_eq!(sso, derive_x25519_keypair_from_entropy(&[0xAB; 16], b"sso"));
	assert_ne!(sso, chat);
	assert_eq!(PublicKey::from(&StaticSecret::from(sso.0)).to_bytes(), sso.1);
	let cross_platform_entropy: [u8; 32] =
		std::array::from_fn(|index| u8::try_from(index + 1).expect("index fits in u8"));
	let (sso_secret, _) = derive_x25519_keypair_from_entropy(&cross_platform_entropy, b"sso");
	let (chat_secret, _) = derive_x25519_keypair_from_entropy(&cross_platform_entropy, b"chat");
	assert_eq!(
		hex::encode(sso_secret),
		"c2c0f3112da0aa390a7d7b644a7f073b80782e49a3240685bc071046d6cc8ad3"
	);
	assert_eq!(
		hex::encode(chat_secret),
		"88705746a9788fb8bc232cc463b243a2828e470b932ce60a16426cc79b4618b1"
	);
}

#[test]
fn generated_bootstrap_uses_matching_x25519_keypair() {
	let generated = create_pairing_bootstrap(&runtime_config()).unwrap();
	assert_eq!(
		PublicKey::from(&StaticSecret::from(generated.encryption_secret_key)).to_bytes(),
		generated.encryption_public_key
	);
	assert_eq!(
		generated.topic,
		bootstrap_topic(generated.statement_store_public_key, generated.encryption_public_key,)
	);
}

#[test]
fn responder_session_mirrors_pairing_session() {
	let host = create_pairing_bootstrap(&runtime_config()).unwrap();
	let (responder_secret, responder_public) =
		derive_x25519_keypair_from_entropy(&[0xAB; 16], b"sso");
	let responder = ResponderIdentity {
		statement_secret: [0x44; 64],
		statement_public_key: [0x55; 32],
		encryption_secret_key: responder_secret,
		encryption_public_key: responder_public,
	};
	let responder_session = establish_responder_session_info(
		&responder,
		host.statement_store_public_key,
		host.encryption_public_key,
	)
	.unwrap();
	let host_session =
		establish_sso_session_info(&host, responder.statement_public_key, responder_public)
			.unwrap();

	assert_eq!(responder_session.session_id_own, host_session.session_id_peer);
	assert_eq!(responder_session.session_id_peer, host_session.session_id_own);
	assert_eq!(responder_session.request_channel, host_session.peer_request_channel);
	assert_eq!(peer_response_channel(&responder_session), host_session.response_channel);

	let data =
		SsoStatementData::Request { request_id: "req-1".to_string(), data: vec![vec![0xde, 0xad]] };
	let encrypted = encrypt_session_statement_data(&responder_session, &data).unwrap();
	assert_eq!(decrypt_session_statement_data(&host_session, &encrypted).unwrap(), data);
}

#[test]
fn session_statement_uses_nonce_ciphertext_tag_framing() {
	let (host_secret, host_public) = x25519_keypair(1);
	let (_, peer_public) = x25519_keypair(2);
	let session =
		establish_sso_session_info(&bootstrap(host_secret, host_public), [0x55; 32], peer_public)
			.unwrap();
	let data =
		SsoStatementData::Request { request_id: "req-1".to_string(), data: vec![vec![0xde, 0xad]] };
	let nonce = [9u8; AEAD_NONCE_LEN];
	let encrypted = encrypt_session_statement_data_with_nonce(&session, &data, nonce).unwrap();

	assert_eq!(&encrypted[..AEAD_NONCE_LEN], nonce);
	assert_eq!(encrypted.len(), AEAD_NONCE_LEN + data.encode().len() + 16);
	assert_eq!(decrypt_session_statement_data(&session, &encrypted).unwrap(), data);
}
