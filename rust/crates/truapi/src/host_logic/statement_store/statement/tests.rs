use super::*;
use crate::host_logic::session::SsoSessionInfo;
use schnorrkel::{ExpansionMode, MiniSecretKey, PublicKey, Signature};

fn test_session() -> SsoSessionInfo {
	let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
	let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
	SsoSessionInfo {
		ss_secret: keypair.secret.to_bytes(),
		ss_public_key: keypair.public.to_bytes(),
		enc_secret: [1; 32],
		peer_enc_pubkey: [2; 32],
		identity_account_id: [3; 32],
		session_id_own: [4; 32],
		session_id_peer: [5; 32],
		request_channel: [6; 32],
		response_channel: [7; 32],
		peer_request_channel: [8; 32],
	}
}

#[test]
fn scure_statement_store_secret_fixture_signs_statement() {
	// Fixture from @novasamatech/statement-store 0.7.5 createSr25519Secret.
	let secret: [u8; 64] = hex::decode(
		"8848ab59e934e06a54835252b8abdf946b893055cd34a4433a32b37470f90745\
         62bb6d70fa3ea98b6ff05ea3f3cac76b62affd2f44c66624873fc6a58e3cd776",
	)
	.unwrap()
	.try_into()
	.unwrap();
	let public_key = statement_public_key_from_secret(secret).unwrap();
	assert_eq!(
		hex::encode(public_key),
		"c0c18f0e3bbb9c0cf31c6c4db37d7d34efb42035daac647a4a14c93320d33857"
	);

	let signed =
		sign_statement_fields(secret, public_key, vec![StatementField::Data(vec![1, 2, 3])])
			.unwrap();
	let verified = decode_verified_statement_data(&signed.encode(), Some(public_key)).unwrap();
	assert_eq!(verified.data, vec![1, 2, 3]);
}

#[test]
fn decodes_statement_data_field() {
	let statement = vec![
		StatementField::Proof(StatementProof::Sr25519 { signature: [1; 64], signer: [2; 32] }),
		StatementField::Expiry(42),
		StatementField::Channel([3; 32]),
		StatementField::Topic1([4; 32]),
		StatementField::Data(vec![0xde, 0xad, 0xbe, 0xef]),
	]
	.encode();

	assert_eq!(decode_statement_data(&statement).unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn signed_statement_scale_round_trips_public_shape() {
	let signed = v01::SignedStatement {
		proof: v01::StatementProof::Sr25519 { signature: [9; 64], signer: [8; 32] },
		decryption_key: Some([7; 32]),
		expiry: Some(99),
		channel: Some([6; 32]),
		topics: vec![[1; 32], [2; 32]],
		data: Some(vec![3, 4, 5]),
	};

	let encoded = signed_statement_to_scale(signed.clone()).unwrap();

	assert_eq!(decode_signed_statement(&encoded).unwrap(), signed);
}

#[test]
fn signing_payload_strips_scale_vec_compact_len() {
	let fields = vec![
		StatementField::Expiry(42),
		StatementField::Channel([3; 32]),
		StatementField::Topic1([4; 32]),
		StatementField::Data(vec![0xde, 0xad, 0xbe, 0xef]),
	];
	let encoded = fields.encode();

	assert_eq!(encoded[0], 16);
	assert_eq!(statement_signing_payload(&fields).unwrap(), encoded[1..]);
}

#[test]
fn validates_unsigned_statement_signing_payload() {
	let payload = unsigned_statement_signing_payload(vec![
		StatementField::Expiry(42),
		StatementField::Topic1([1; 32]),
		StatementField::Topic2([2; 32]),
		StatementField::Data(vec![0xde, 0xad]),
	])
	.unwrap();

	validate_unsigned_statement_signing_payload(&payload).unwrap();
}

#[test]
fn unsigned_statement_signing_payload_rejects_transaction_preimage() {
	let tx_preimage = vec![0x00, 0x00, 0x01, 0x02, 0x03];

	let err = validate_unsigned_statement_signing_payload(&tx_preimage).unwrap_err();

	assert!(matches!(err, StatementSigningPayloadError::InvalidScale(_)));
	assert!(err.to_string().contains("invalid statement signing payload"));
}

#[test]
fn unsigned_statement_signing_payload_rejects_proof_field() {
	let payload = statement_signing_payload(&[StatementField::Proof(StatementProof::Sr25519 {
		signature: [1; 64],
		signer: [2; 32],
	})])
	.unwrap();

	assert!(matches!(
		validate_unsigned_statement_signing_payload(&payload).unwrap_err(),
		StatementSigningPayloadError::ContainsProof
	));
}

#[test]
fn unsigned_statement_signing_payload_rejects_non_contiguous_topics() {
	let payload = statement_signing_payload(&[
		StatementField::Topic1([1; 32]),
		StatementField::Topic3([3; 32]),
	])
	.unwrap();

	assert!(matches!(
		validate_unsigned_statement_signing_payload(&payload).unwrap_err(),
		StatementSigningPayloadError::NonContiguousTopics
	));
}

#[test]
fn builds_signed_session_request_statement() {
	let session = test_session();

	let statement = build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();
	let mut input = statement.as_slice();
	let fields = Vec::<StatementField>::decode(&mut input).unwrap();

	assert!(input.is_empty());
	assert_eq!(fields.len(), 5);
	let StatementField::Proof(StatementProof::Sr25519 { signature, signer }) = fields[0] else {
		panic!("expected sr25519 proof");
	};
	assert_eq!(signer, session.ss_public_key);
	assert_eq!(fields[1], StatementField::Expiry(42));
	assert_eq!(fields[2], StatementField::Channel(session.request_channel));
	assert_eq!(fields[3], StatementField::Topic1(session.session_id_own));
	assert_eq!(fields[4], StatementField::Data(vec![0xde, 0xad]));

	let payload = statement_signing_payload(&fields[1..]).unwrap();
	let public = PublicKey::from_bytes(&signer).unwrap();
	let signature = Signature::from_bytes(&signature).unwrap();
	public.verify_simple(SR25519_SIGNING_CONTEXT, &payload, &signature).unwrap();
}

#[test]
fn verified_statement_data_accepts_valid_sr25519_proof() {
	let session = test_session();
	let statement = build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();

	let verified = decode_verified_statement_data(&statement, Some(session.ss_public_key)).unwrap();

	assert_eq!(
		verified,
		VerifiedStatementData {
			data: vec![0xde, 0xad],
			signer: session.ss_public_key,
			expiry: Some(42),
		}
	);
}

#[test]
fn verified_statement_data_rejects_tampered_signature() {
	let session = test_session();
	let statement = build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();
	let mut fields = Vec::<StatementField>::decode(&mut statement.as_slice()).unwrap();
	let StatementField::Proof(StatementProof::Sr25519 { signature, .. }) = &mut fields[0] else {
		panic!("expected sr25519 proof");
	};
	signature[0] ^= 0xff;

	let err =
		decode_verified_statement_data(&fields.encode(), Some(session.ss_public_key)).unwrap_err();

	assert!(
		matches!(err, StatementStoreParseError::InvalidStatementProof(reason) if reason.contains("signature verification failed"))
	);
}

#[test]
fn verified_statement_data_rejects_wrong_expected_signer() {
	let session = test_session();
	let statement = build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();

	assert_eq!(
		decode_verified_statement_data(&statement, Some([0xaa; 32])).unwrap_err(),
		StatementStoreParseError::InvalidStatementProof(
			"statement proof signer does not match expected peer".to_string()
		)
	);
}

#[test]
fn signing_rejects_mismatched_session_key_material() {
	let mut session = test_session();
	session.ss_public_key = [0xff; 32];

	assert_eq!(
		build_signed_session_request_statement(&session, vec![0xde], 42).unwrap_err(),
		"ss_secret does not match session statement public key"
	);
}

#[test]
fn signing_rejects_already_signed_statements() {
	let session = test_session();
	let fields = vec![StatementField::Proof(StatementProof::Sr25519 {
		signature: [1; 64],
		signer: session.ss_public_key,
	})];

	assert_eq!(
		sign_statement_fields(session.ss_secret, session.ss_public_key, fields).unwrap_err(),
		"statement is already signed"
	);
}

#[test]
fn rejects_statement_without_data_field() {
	let statement = vec![StatementField::Expiry(42)].encode();

	assert_eq!(
		decode_statement_data(&statement).unwrap_err(),
		StatementStoreParseError::Malformed("statement has no data".to_string())
	);
}
