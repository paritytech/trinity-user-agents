use parity_scale_codec::{Compact, Decode, Encode};
use schnorrkel::{PublicKey, Signature};
use thiserror::Error;
use truapi::v01;

use super::StatementStoreParseError;
use crate::{
	host_internal::extrinsic::sr25519_secret_from_bytes,
	host_logic::{product_account::SR25519_SIGNING_CONTEXT, session::SsoSessionInfo},
};

/// Error validating an exact unsigned statement-store signing payload.
#[derive(Debug, Error)]
pub enum StatementSigningPayloadError {
	/// Payload bytes are not a sequence of SCALE statement fields.
	#[error("invalid statement signing payload: {0}")]
	InvalidScale(#[from] parity_scale_codec::Error),
	/// Payload contains no fields.
	#[error("statement signing payload has no fields")]
	Empty,
	/// Payload contains a proof field, but only unsigned payloads are accepted.
	#[error("statement signing payload must not include proof")]
	ContainsProof,
	/// Payload contains the same singleton field more than once.
	#[error("statement signing payload has duplicate {field}")]
	DuplicateField {
		/// Duplicated field name.
		field: &'static str,
	},
	/// Topic fields are not Topic1, Topic2, ... without gaps.
	#[error("statement signing payload topics are not contiguous")]
	NonContiguousTopics,
	/// Payload decodes to a statement but is not the canonical field encoding.
	#[error("statement signing payload is not canonical")]
	NonCanonical,
}

/// Verified statement payload plus the sr25519 signer recovered from proof.
///
/// SSO receivers verify incoming statement proofs against the paired
/// `identityAccountId` as required by host-spec B.3.4:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L147-L149>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedStatementData {
	/// Raw statement data field.
	pub data: Vec<u8>,
	/// Sr25519 signer recovered from the proof.
	pub signer: [u8; 32],
	/// Raw `Expiry` field, if present: unix seconds in the upper 32 bits.
	pub expiry: Option<u64>,
}

/// SCALE statement proof variants mirrored from `sp_statement_store::Proof`.
///
/// See the current upstream `Proof` codec:
/// <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/lib.rs#L273-L299>
///
/// `OnChain` is retained for v01 wire compatibility with older
/// statement-store bytes:
/// <https://github.com/paritytech/polkadot-sdk/blob/7d525248d594c79dcc5e30217becbd56d2fcda40/substrate/primitives/statement-store/src/lib.rs#L260-L289>
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementProof {
	/// Statement signed with an sr25519 key.
	Sr25519 {
		/// Signature over the sorted non-proof fields.
		signature: [u8; 64],
		/// sr25519 public key that produced the signature.
		signer: [u8; 32],
	},
	/// Statement signed with an ed25519 key.
	Ed25519 {
		/// Signature over the sorted non-proof fields.
		signature: [u8; 64],
		/// ed25519 public key that produced the signature.
		signer: [u8; 32],
	},
	/// Statement signed with an ECDSA secp256k1 key.
	Ecdsa {
		/// Signature over the sorted non-proof fields.
		signature: [u8; 65],
		/// Compressed secp256k1 public key that produced the signature.
		signer: [u8; 33],
	},
	/// Statement anchored by a pallet-statement chain event instead of a signature.
	OnChain {
		/// Account that submitted the statement on chain.
		who: [u8; 32],
		/// Block containing the statement event.
		block_hash: [u8; 32],
		/// Event index within that block.
		event: u64,
	},
}

/// SCALE statement field variants mirrored from `sp_statement_store::Field`.
///
/// See the upstream statement field vector codec:
/// <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/lib.rs#L314-L337>
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementField {
	/// Authenticity proof over the other fields.
	Proof(StatementProof),
	/// Identifier of the key able to decrypt the `Data` field.
	DecryptionKey([u8; 32]),
	/// Expiry, unix seconds in the upper 32 bits.
	Expiry(u64),
	/// Per-signer channel; the store keeps one live statement per channel.
	Channel([u8; 32]),
	/// First matching topic.
	Topic1([u8; 32]),
	/// Second matching topic.
	Topic2([u8; 32]),
	/// Third matching topic.
	Topic3([u8; 32]),
	/// Fourth matching topic.
	Topic4([u8; 32]),
	/// Statement payload bytes.
	Data(Vec<u8>),
}

/// Extract the raw `Data` field from a SCALE-encoded statement.
pub fn decode_statement_data(statement: &[u8]) -> Result<Vec<u8>, StatementStoreParseError> {
	statement_data_from_fields(decode_statement_fields(statement)?)
}

/// Verify statement proof and extract signer, expiry, and raw `Data` field.
pub fn decode_verified_statement_data(
	statement: &[u8],
	expected_signer: Option<[u8; 32]>,
) -> Result<VerifiedStatementData, StatementStoreParseError> {
	let fields = decode_statement_fields(statement)?;
	let signer = verify_statement_proof(&fields, expected_signer)?;
	let expiry = fields.iter().find_map(|field| match field {
		StatementField::Expiry(value) => Some(*value),
		_ => None,
	});
	let data = statement_data_from_fields(fields)?;
	Ok(VerifiedStatementData { data, signer, expiry })
}

/// Whether a statement `Expiry` field (unix seconds in the upper 32 bits) is
/// in the past relative to `now_unix_secs`.
pub fn statement_expiry_elapsed(expiry: u64, now_unix_secs: u64) -> bool {
	(expiry >> 32) < now_unix_secs
}

/// Decode a SCALE signed statement into the public v01 statement shape.
pub fn decode_signed_statement(
	statement: &[u8],
) -> Result<v01::SignedStatement, StatementStoreParseError> {
	signed_statement_from_fields(decode_statement_fields(statement)?)
}

/// Build a signed statement on the active SSO request channel.
///
/// Host-spec B.3.1 defines directional session topics; B.4.1 defines the
/// encrypted statement `data` wrapper carried by this statement:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L119-L131>
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L151-L175>
pub fn build_signed_session_request_statement(
	session: &SsoSessionInfo,
	encrypted_data: Vec<u8>,
	expiry: u64,
) -> Result<Vec<u8>, String> {
	build_signed_statement(
		session,
		session.request_channel,
		session.session_id_own,
		encrypted_data,
		expiry,
	)
}

/// Build a signed statement for an arbitrary channel/topic pair.
pub fn build_signed_statement(
	session: &SsoSessionInfo,
	channel: [u8; 32],
	topic1: [u8; 32],
	data: Vec<u8>,
	expiry: u64,
) -> Result<Vec<u8>, String> {
	let fields = vec![
		StatementField::Expiry(expiry),
		StatementField::Channel(channel),
		StatementField::Topic1(topic1),
		StatementField::Data(data),
	];
	sign_statement_fields(session.ss_secret, session.ss_public_key, fields)
		.map(|fields| fields.encode())
}

/// Sort fields, insert an sr25519 proof, and return signed fields.
pub fn sign_statement_fields(
	ss_secret: [u8; 64],
	expected_public_key: [u8; 32],
	mut fields: Vec<StatementField>,
) -> Result<Vec<StatementField>, String> {
	if fields.iter().any(|field| matches!(field, StatementField::Proof(_))) {
		return Err("statement is already signed".to_string());
	}
	fields.sort_by_key(statement_field_sort_index);

	let secret = sr25519_secret_from_bytes(&ss_secret)
		.map_err(|reason| format!("invalid ss_secret: {reason}"))?;
	let public = secret.to_public();
	if public.to_bytes() != expected_public_key {
		return Err("ss_secret does not match session statement public key".to_string());
	}

	let signing_payload = statement_signing_payload(&fields)?;
	let signature = secret
		.sign_simple(SR25519_SIGNING_CONTEXT, &signing_payload, &public)
		.to_bytes();

	let mut signed = Vec::with_capacity(fields.len() + 1);
	signed.push(StatementField::Proof(StatementProof::Sr25519 {
		signature,
		signer: expected_public_key,
	}));
	signed.extend(fields);
	Ok(signed)
}

/// Derive the sr25519 public key for a 64-byte statement-store secret.
pub fn statement_public_key_from_secret(ss_secret: [u8; 64]) -> Result<[u8; 32], String> {
	let secret = sr25519_secret_from_bytes(&ss_secret)
		.map_err(|reason| format!("invalid ss_secret: {reason}"))?;
	Ok(secret.to_public().to_bytes())
}

/// Build the statement proof payload for unsigned fields.
pub fn unsigned_statement_signing_payload(
	mut fields: Vec<StatementField>,
) -> Result<Vec<u8>, String> {
	if fields.iter().any(|field| matches!(field, StatementField::Proof(_))) {
		return Err("statement is already signed".to_string());
	}
	fields.sort_by_key(statement_field_sort_index);
	statement_signing_payload(&fields)
}

/// Validate an exact unsigned statement signing payload.
///
/// Statement proof signing intentionally signs the encoded statement fields
/// without the surrounding SCALE vector length. Accept only payloads that
/// round-trip from the public unsigned statement shape, so a paired peer cannot
/// reuse the statement-signing path as a generic product-account signing oracle.
pub fn validate_unsigned_statement_signing_payload(
	payload: &[u8],
) -> Result<(), StatementSigningPayloadError> {
	let fields = decode_unsigned_statement_signing_payload_fields(payload)?;
	if fields.is_empty() {
		return Err(StatementSigningPayloadError::Empty);
	}
	let statement = unsigned_statement_from_fields(fields)?;
	let fields = statement_fields_from_v01(statement)
		.expect("unsigned_statement_from_fields emits at most four topics and no proof; qed");
	let canonical = unsigned_statement_signing_payload(fields).expect(
		"statement_fields_from_v01 emitted unsigned fields, and SCALE vector encoding is decodable; qed",
	);
	if canonical != payload {
		return Err(StatementSigningPayloadError::NonCanonical);
	}
	Ok(())
}

/// Build the statement signing payload from sorted fields.
pub fn statement_signing_payload(fields: &[StatementField]) -> Result<Vec<u8>, String> {
	let encoded = fields.to_vec().encode();
	let mut input = encoded.as_slice();
	let _: Compact<u32> =
		Decode::decode(&mut input).map_err(|err| format!("invalid statement vector: {err}"))?;
	let compact_len = encoded.len() - input.len();
	Ok(encoded[compact_len..].to_vec())
}

fn decode_unsigned_statement_signing_payload_fields(
	mut input: &[u8],
) -> Result<Vec<StatementField>, StatementSigningPayloadError> {
	let mut fields = Vec::new();
	while !input.is_empty() {
		fields.push(StatementField::decode(&mut input)?);
	}
	Ok(fields)
}

fn decode_statement_fields(
	statement: &[u8],
) -> Result<Vec<StatementField>, StatementStoreParseError> {
	let mut input = statement;
	let fields: Vec<StatementField> = Decode::decode(&mut input)
		.map_err(|err| StatementStoreParseError::InvalidStatementScale(err.to_string()))?;
	if !input.is_empty() {
		return Err(StatementStoreParseError::Malformed(
			"statement has trailing bytes".to_string(),
		));
	}
	Ok(fields)
}

fn statement_data_from_fields(
	fields: Vec<StatementField>,
) -> Result<Vec<u8>, StatementStoreParseError> {
	fields
		.into_iter()
		.find_map(|field| match field {
			StatementField::Data(value) => Some(value),
			_ => None,
		})
		.ok_or_else(|| StatementStoreParseError::Malformed("statement has no data".to_string()))
}

fn verify_statement_proof(
	fields: &[StatementField],
	expected_signer: Option<[u8; 32]>,
) -> Result<[u8; 32], StatementStoreParseError> {
	let mut proof = None;
	let mut unsigned_fields = Vec::with_capacity(fields.len().saturating_sub(1));
	for field in fields {
		match field {
			StatementField::Proof(StatementProof::Sr25519 { signature, signer }) => {
				if proof.replace((*signature, *signer)).is_some() {
					return Err(StatementStoreParseError::InvalidStatementProof(
						"statement has duplicate proof".to_string(),
					));
				}
			},
			StatementField::Proof(_) => {
				return Err(StatementStoreParseError::InvalidStatementProof(
					"statement proof is not sr25519".to_string(),
				));
			},
			field => unsigned_fields.push(field.clone()),
		}
	}
	let (signature, signer) = proof.ok_or_else(|| {
		StatementStoreParseError::InvalidStatementProof("statement has no proof".to_string())
	})?;
	if let Some(expected) = expected_signer &&
		signer != expected
	{
		return Err(StatementStoreParseError::InvalidStatementProof(
			"statement proof signer does not match expected peer".to_string(),
		));
	}

	unsigned_fields.sort_by_key(statement_field_sort_index);
	let payload =
		statement_signing_payload(&unsigned_fields).map_err(StatementStoreParseError::Malformed)?;
	let public = PublicKey::from_bytes(&signer).map_err(|err| {
		StatementStoreParseError::InvalidStatementProof(format!("invalid sr25519 signer: {err}"))
	})?;
	let signature = Signature::from_bytes(&signature).map_err(|err| {
		StatementStoreParseError::InvalidStatementProof(format!("invalid sr25519 signature: {err}"))
	})?;
	public
		.verify_simple(SR25519_SIGNING_CONTEXT, &payload, &signature)
		.map_err(|err| {
			StatementStoreParseError::InvalidStatementProof(format!(
				"sr25519 signature verification failed: {err}"
			))
		})?;
	Ok(signer)
}

fn unsigned_statement_from_fields(
	fields: Vec<StatementField>,
) -> Result<v01::Statement, StatementSigningPayloadError> {
	let mut decryption_key = None;
	let mut expiry = None;
	let mut channel = None;
	let mut topics = Vec::new();
	let mut data = None;

	for field in fields {
		match field {
			StatementField::Proof(_) => {
				return Err(StatementSigningPayloadError::ContainsProof);
			},
			StatementField::DecryptionKey(value) => {
				if decryption_key.replace(value).is_some() {
					return Err(StatementSigningPayloadError::DuplicateField {
						field: "decryption key",
					});
				}
			},
			StatementField::Expiry(value) => {
				if expiry.replace(value).is_some() {
					return Err(StatementSigningPayloadError::DuplicateField { field: "expiry" });
				}
			},
			StatementField::Channel(value) => {
				if channel.replace(value).is_some() {
					return Err(StatementSigningPayloadError::DuplicateField { field: "channel" });
				}
			},
			StatementField::Topic1(value) => push_unsigned_statement_topic(&mut topics, 0, value)?,
			StatementField::Topic2(value) => push_unsigned_statement_topic(&mut topics, 1, value)?,
			StatementField::Topic3(value) => push_unsigned_statement_topic(&mut topics, 2, value)?,
			StatementField::Topic4(value) => push_unsigned_statement_topic(&mut topics, 3, value)?,
			StatementField::Data(value) => {
				if data.replace(value).is_some() {
					return Err(StatementSigningPayloadError::DuplicateField { field: "data" });
				}
			},
		}
	}

	Ok(v01::Statement { proof: None, decryption_key, expiry, channel, topics, data })
}

fn push_unsigned_statement_topic(
	topics: &mut Vec<[u8; 32]>,
	expected_index: usize,
	value: [u8; 32],
) -> Result<(), StatementSigningPayloadError> {
	if topics.len() != expected_index {
		return Err(StatementSigningPayloadError::NonContiguousTopics);
	}
	topics.push(value);
	Ok(())
}

/// Convert a public v01 statement into SCALE statement fields.
pub fn statement_fields_from_v01(statement: v01::Statement) -> Result<Vec<StatementField>, String> {
	let mut fields = Vec::new();
	if let Some(proof) = statement.proof {
		fields.push(StatementField::Proof(proof.into()));
	}
	if let Some(decryption_key) = statement.decryption_key {
		fields.push(StatementField::DecryptionKey(decryption_key));
	}
	if let Some(expiry) = statement.expiry {
		fields.push(StatementField::Expiry(expiry));
	}
	if let Some(channel) = statement.channel {
		fields.push(StatementField::Channel(channel));
	}
	push_statement_topics(&mut fields, statement.topics)?;
	if let Some(data) = statement.data {
		fields.push(StatementField::Data(data));
	}
	Ok(fields)
}

/// Convert a public v01 signed statement into SCALE bytes.
pub fn signed_statement_to_scale(statement: v01::SignedStatement) -> Result<Vec<u8>, String> {
	let mut fields = vec![StatementField::Proof(statement.proof.into())];
	if let Some(decryption_key) = statement.decryption_key {
		fields.push(StatementField::DecryptionKey(decryption_key));
	}
	if let Some(expiry) = statement.expiry {
		fields.push(StatementField::Expiry(expiry));
	}
	if let Some(channel) = statement.channel {
		fields.push(StatementField::Channel(channel));
	}
	push_statement_topics(&mut fields, statement.topics)?;
	if let Some(data) = statement.data {
		fields.push(StatementField::Data(data));
	}
	fields.sort_by_key(statement_field_sort_index);
	Ok(fields.encode())
}

/// Convert decoded statement fields into the public signed-statement shape.
fn signed_statement_from_fields(
	fields: Vec<StatementField>,
) -> Result<v01::SignedStatement, StatementStoreParseError> {
	let mut proof = None;
	let mut decryption_key = None;
	let mut expiry = None;
	let mut channel = None;
	let mut topics = Vec::new();
	let mut data = None;

	for field in fields {
		match field {
			StatementField::Proof(value) => {
				if proof.replace(value.into()).is_some() {
					return Err(StatementStoreParseError::Malformed(
						"statement has duplicate proof".to_string(),
					));
				}
			},
			StatementField::DecryptionKey(value) => {
				if decryption_key.replace(value).is_some() {
					return Err(StatementStoreParseError::Malformed(
						"statement has duplicate decryption key".to_string(),
					));
				}
			},
			StatementField::Expiry(value) => {
				if expiry.replace(value).is_some() {
					return Err(StatementStoreParseError::Malformed(
						"statement has duplicate expiry".to_string(),
					));
				}
			},
			StatementField::Channel(value) => {
				if channel.replace(value).is_some() {
					return Err(StatementStoreParseError::Malformed(
						"statement has duplicate channel".to_string(),
					));
				}
			},
			StatementField::Topic1(value) |
			StatementField::Topic2(value) |
			StatementField::Topic3(value) |
			StatementField::Topic4(value) => topics.push(value),
			StatementField::Data(value) => {
				if data.replace(value).is_some() {
					return Err(StatementStoreParseError::Malformed(
						"statement has duplicate data".to_string(),
					));
				}
			},
		}
	}

	let proof = proof
		.ok_or_else(|| StatementStoreParseError::Malformed("statement has no proof".to_string()))?;
	Ok(v01::SignedStatement { proof, decryption_key, expiry, channel, topics, data })
}

/// Convert an internal proof into the public v01 proof shape.
pub fn statement_proof_to_v01(proof: StatementProof) -> v01::StatementProof {
	proof.into()
}

impl From<StatementProof> for v01::StatementProof {
	fn from(proof: StatementProof) -> Self {
		match proof {
			StatementProof::Sr25519 { signature, signer } => {
				v01::StatementProof::Sr25519 { signature, signer }
			},
			StatementProof::Ed25519 { signature, signer } => {
				v01::StatementProof::Ed25519 { signature, signer }
			},
			StatementProof::Ecdsa { signature, signer } => {
				v01::StatementProof::Ecdsa { signature, signer }
			},
			StatementProof::OnChain { who, block_hash, event } => {
				v01::StatementProof::OnChain { who, block_hash, event }
			},
		}
	}
}

impl From<v01::StatementProof> for StatementProof {
	fn from(proof: v01::StatementProof) -> Self {
		match proof {
			v01::StatementProof::Sr25519 { signature, signer } => {
				StatementProof::Sr25519 { signature, signer }
			},
			v01::StatementProof::Ed25519 { signature, signer } => {
				StatementProof::Ed25519 { signature, signer }
			},
			v01::StatementProof::Ecdsa { signature, signer } => {
				StatementProof::Ecdsa { signature, signer }
			},
			v01::StatementProof::OnChain { who, block_hash, event } => {
				StatementProof::OnChain { who, block_hash, event }
			},
		}
	}
}

fn push_statement_topics(
	fields: &mut Vec<StatementField>,
	topics: Vec<[u8; 32]>,
) -> Result<(), String> {
	if topics.len() > 4 {
		return Err(format!("statement has {} topics, maximum is 4", topics.len()));
	}
	for (index, topic) in topics.into_iter().enumerate() {
		fields.push(match index {
			0 => StatementField::Topic1(topic),
			1 => StatementField::Topic2(topic),
			2 => StatementField::Topic3(topic),
			3 => StatementField::Topic4(topic),
			_ => unreachable!("topic count checked above"),
		});
	}
	Ok(())
}

fn statement_field_sort_index(field: &StatementField) -> u8 {
	// Keep in sync with upstream `sp_statement_store::Field` discriminants:
	// https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/lib.rs#L314-L337
	match field {
		StatementField::Proof(_) => 0,
		StatementField::DecryptionKey(_) => 1,
		StatementField::Expiry(_) => 2,
		StatementField::Channel(_) => 3,
		StatementField::Topic1(_) => 4,
		StatementField::Topic2(_) => 5,
		StatementField::Topic3(_) => 6,
		StatementField::Topic4(_) => 7,
		StatementField::Data(_) => 8,
	}
}

/// Format a 32-byte statement-store topic as `0x`-prefixed hex.
pub fn hex_topic(topic: &[u8; 32]) -> String {
	format!("0x{}", hex::encode(topic))
}

#[cfg(test)]
mod tests;
