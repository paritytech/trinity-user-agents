//! SCALE codecs for host-papp SSO session-channel messages.
//!
//! These are the encrypted payloads carried inside statement-store
//! `SsoStatementData::Request` / `Response` frames. A pairing host sends a
//! request with [`RemoteMessage::request`]; each wire response carries a
//! [`Response`] envelope. Annotated handler signatures pair requests with their
//! response variants (see `sso::wire`).
//! The encrypted statement envelope and message identifiers are specified in
//! host-spec:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L151-L183>
//! The baseline remote message catalog is specified in host-spec:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L194-L208>
//! Deployed extension variants are tracked as a host-spec divergence:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/divergences.md?plain=1#L26-L33>
//! Field order and enum variant order are kept wire-compatible with
//! `@novasamatech/host-papp` 0.8.11:
//! <https://github.com/paritytech/triangle-js-sdks/blob/afb26e2c78bf1134886c1248c1bf2b6b4dc1fce9/packages/host-papp/src/sso/sessionManager/scale/remoteMessage.ts>
//! <https://github.com/paritytech/triangle-js-sdks/blob/afb26e2c78bf1134886c1248c1bf2b6b4dc1fce9/packages/host-papp/src/sso/sessionManager/scale/signing.ts>
//! <https://github.com/paritytech/triangle-js-sdks/blob/afb26e2c78bf1134886c1248c1bf2b6b4dc1fce9/packages/host-papp/src/sso/sessionManager/scale/ringVrf.ts>
//! <https://github.com/paritytech/triangle-js-sdks/blob/afb26e2c78bf1134886c1248c1bf2b6b4dc1fce9/packages/host-papp/src/sso/sessionManager/scale/resourceAllocation.ts>
//! <https://github.com/paritytech/triangle-js-sdks/blob/afb26e2c78bf1134886c1248c1bf2b6b4dc1fce9/packages/host-papp/src/sso/sessionManager/scale/createTransaction.ts>

use core::fmt;

use parity_scale_codec::{Decode, Encode};
use truapi::latest::{
	AllocatableResource, HostAccountCreateProofResponse, HostAccountGetAliasResponse,
	HostAccountSignVrfError, HostSignPayloadRequest, HostSignPayloadResponse, HostSignRawRequest,
	LegacyAccountTxPayload, ProductAccountId, ProductAccountTxPayload, RawPayload,
	RegisteredRingVrfKey, TxPayloadExtension, VrfSignature,
};

#[cfg(test)]
use crate::host_logic::{
	sso::pairing::encrypt_session_statement_data_with_nonce, x25519_seal::AEAD_NONCE_LEN,
};
use crate::{
	host_logic::{
		session::SsoSessionInfo,
		sso::pairing::{
			SsoStatementData, decrypt_session_statement_data, encrypt_session_statement_data,
			peer_response_channel,
		},
		statement_store::{
			build_signed_session_request_statement, build_signed_statement,
			decode_verified_statement_data, statement_expiry_elapsed,
		},
	},
	platform::ProductId,
	unix_time::current_unix_secs,
};

pub mod v1;

/// Transport-level acknowledgement code for an SSO session statement.
#[derive(Clone, Encode, Decode, derive_more::Display)]
pub enum SsoResponseCode {
	/// The request statement was decrypted and decoded.
	#[codec(index = 0)]
	#[display("success")]
	Success,
	/// The request statement could not be decrypted.
	#[codec(index = 1)]
	#[display("decryptionFailed")]
	DecryptionFailed,
	/// The request statement decrypted but its messages could not be decoded.
	#[codec(index = 2)]
	#[display("decodingFailed")]
	DecodingFailed,
}

impl TryFrom<u8> for SsoResponseCode {
	type Error = ();

	fn try_from(value: u8) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(Self::Success),
			1 => Ok(Self::DecryptionFailed),
			2 => Ok(Self::DecodingFailed),
			_ => Err(()),
		}
	}
}

/// Top-level remote message sent over the encrypted SSO channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteMessage {
	/// Correlation id used to match signing-host responses to pairing-host requests.
	pub message_id: String,
	/// Versioned remote message body.
	pub data: RemoteMessageData,
}

/// Versioned remote message body.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteMessageData {
	/// Version 1 of the remote message catalog.
	V1(v1::RemoteMessage),
}

/// A response payload addressed to the request it answers.
///
/// SCALE encodes the correlation id before the payload for every response variant.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Response<P> {
	/// `message_id` of the request being answered.
	pub responding_to: String,
	/// The operation's result, without transport metadata.
	pub payload: P,
}

/// Result of answering an SSO message for a caller-owned session.
#[derive(Debug, Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum SsoRequestOutcome {
	/// Encoded response to post on the session.
	Response {
		/// SCALE-encoded `RemoteMessage` response.
		message: Vec<u8>,
	},
	/// The peer ended the session.
	Disconnected,
	/// The message did not require a response.
	Ignored,
}

/// A product's canonical request payload and the identity of its caller.
///
/// SCALE encodes the caller followed directly by the payload fields.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductRequest<P> {
	/// Product making the request.
	pub calling_product_id: ProductId,
	/// Canonical payload sent to the signing host.
	pub payload: P,
}

/// Signing request flavor sent to the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SignRequest {
	/// Sign a full Substrate extrinsic payload.
	Payload(Box<HostSignPayloadRequest>),
	/// Sign raw bytes or a string message.
	Raw(HostSignRawRequest),
	/// Temporary unwatermarked product signing (#612). Appended so existing
	/// signing requests keep their SCALE discriminants and semantics.
	RawUnwatermarkedDeprecated(HostSignRawRequest),
	/// Temporary unwatermarked legacy signing (#612). Older peers reject this
	/// variant rather than silently signing a different, watermarked payload.
	RawWithLegacyAccountUnwatermarkedDeprecated(SignRawWithLegacyAccountRequest),
}

/// Request sent when a product asks the paired signing host to sign raw data with a
/// user-imported legacy account.
///
/// Unlike product-account signing, the signer is the raw account id selected
/// from the user's legacy accounts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignRawWithLegacyAccountRequest {
	/// Legacy account that signs the payload.
	pub account: [u8; 32],
	/// Raw bytes or string message to sign.
	pub data: RawPayload,
}

/// Response returned by the signing host for a product-account signing request.
///
/// Decoded from [`v1::RemoteMessage::SignResponse`] while the runtime is waiting
/// for a matching SSO remote message id.
pub type SignResponse = Result<HostSignPayloadResponse, String>;

/// Response returned by the signing host for a legacy-account raw signing request.
///
/// Decoded from [`v1::RemoteMessage::SignRawWithLegacyAccountResponse`] and mapped back to
/// the public raw-signing response shape.
pub type SignRawWithLegacyAccountResponse = Result<Vec<u8>, String>;

/// RFC-0023 VRF-signing response returned by the Account Holder.
pub type SignVrfResponse = Result<VrfSignature, HostAccountSignVrfError>;

/// Failure returned by the Account Holder for RFC-0024 ring-VRF operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
pub enum RingVrfError {
	/// The `RingLocation` did not resolve to a known ring.
	RingNotFound,
	/// The registered member key is not a member of the requested ring.
	NotMember,
	/// The requested key handle is not registered.
	KeyNotRegistered,
	/// The requested key handle is not registered for the requested ring.
	KeyNotInRing,
	/// The foreign key owner has not allowlisted the caller.
	NotAllowlisted,
	/// User or Account Holder rejected the request.
	Rejected,
	/// Catch-all failure, carrying a diagnostic reason.
	#[display("Unknown: {reason}")]
	Unknown {
		/// Diagnostic failure description.
		reason: String,
	},
}

/// Response returned by the Account Holder for a ring-VRF alias request.
pub type GetAccountAliasResponse = Result<HostAccountGetAliasResponse, RingVrfError>;

/// Response returned by the Account Holder for key registration.
pub type RegisterRingVrfKeyResponse = Result<[u8; 32], RingVrfError>;

/// Response returned by the Account Holder for registry listing.
pub type ListRingVrfKeysResponse = Result<Vec<RegisteredRingVrfKey>, RingVrfError>;

/// Response returned by the Account Holder for direct ring-VRF signing.
pub type RingVrfSignResponse = Result<Vec<u8>, RingVrfError>;

/// Response returned by the Account Holder for a ring-VRF proof request.
pub type CreateAccountProofResponse = Result<HostAccountCreateProofResponse, RingVrfError>;

/// Request sent when a product asks the signing host to allocate SSO-backed
/// resources.
///
/// Used by `ResourceAllocation::request` for capabilities from
/// `docs/rfcs/0010-allowance.md`, such as statement-store allowance and
/// auto-signing material.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourceAllocationRequest {
	/// Product id the allocation is requested for.
	pub calling_product_id: ProductId,
	/// Resources to allocate; outcomes come back in the same order.
	pub resources: Vec<AllocatableResource>,
	/// Policy applied when an allocation already exists for this product.
	pub on_existing: OnExistingAllowancePolicy,
}

/// Signing-host policy for already-existing resource allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum OnExistingAllowancePolicy {
	/// Return the existing allocation unchanged; allocate only if none exists.
	Ignore,
	/// Assign one additional slot to the existing allowance account.
	Increase,
}

/// Response returned by the signing host for a resource-allocation request.
pub type ResourceAllocationResponse = Result<Vec<SsoAllocationOutcome>, String>;

/// Per-resource allocation result from the signing host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocationOutcome {
	/// Resource granted, carrying the allocated material.
	Allocated(SsoAllocatedResource),
	/// User or signing host declined this resource.
	Rejected,
	/// Signing host cannot currently grant this resource.
	NotAvailable,
}

/// Resource material allocated by the signing host.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoAllocatedResource {
	/// Statement Store slot allowance material.
	StatementStoreAllowance {
		/// Private key of the allowance account assigned to the slot.
		slot_account_key: Vec<u8>,
	},
	/// Bulletin chain slot allowance material.
	BulletinAllowance {
		/// Private key of the allowance account assigned to the slot.
		slot_account_key: Vec<u8>,
	},
	/// Smart-contract allowance carries no key material.
	SmartContractAllowance,
	/// Auto-signing material for the product subtree.
	AutoSigning {
		/// Private key of the product subtree root.
		product_root_private_key: [u8; 64],
		/// Entropy of the product's ring-VRF domain.
		ring_vrf_domain_entropy: [u8; 32],
	},
}

impl SsoAllocatedResource {
	/// Stable, non-secret resource discriminant suitable for errors and logs.
	pub const fn kind(&self) -> &'static str {
		match self {
			Self::StatementStoreAllowance { .. } => "statement-store-allowance",
			Self::BulletinAllowance { .. } => "bulletin-allowance",
			Self::SmartContractAllowance => "smart-contract-allowance",
			Self::AutoSigning { .. } => "auto-signing",
		}
	}
}

impl fmt::Debug for SsoAllocatedResource {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.kind())
	}
}

/// A pairing host withdrawing a request it published and no longer waits on.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Withdrawal {
	/// `message_id` of the withdrawn request.
	pub message_id: String,
}

/// Consent-free request for `//product//{product_id}`'s sr25519 public key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductSubtreeRequest {
	/// DotNS product identifier whose hard subtree is requested.
	pub product_id: ProductId,
}

/// Account Holder response carrying a product subtree public key.
pub type ProductSubtreeResponse = Result<[u8; 32], String>;

/// Request sent when a product asks the signing host to create a transaction
/// for a product-derived account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionRequest {
	/// Transaction payload to build.
	pub payload: CreateTransactionPayload,
}

/// Versioned transaction-creation payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionPayload {
	/// Version 1 product-account payload.
	V1(SsoProductTxPayload),
}

/// A product-account transaction as it crosses to the signing host.
///
/// [`ProductAccountTxPayload`] without `contacts`: the pairing host swaps every
/// declared handle for its account before relaying, so the signing host has no
/// handle to resolve, and the wire stays the shape host-papp and existing
/// wallets encode.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SsoProductTxPayload {
	/// Product account that signs.
	pub signer: ProductAccountId,
	/// Chain the transaction is for.
	pub genesis_hash: [u8; 32],
	/// SCALE-encoded call, contacts already substituted.
	pub call_data: Vec<u8>,
	/// Extensions the caller supplied.
	pub extensions: Vec<TxPayloadExtension>,
	/// Transaction extension version.
	pub tx_ext_version: u8,
}

impl SsoProductTxPayload {
	/// The relayed form of a payload whose contacts are already substituted.
	pub fn from_resolved(payload: ProductAccountTxPayload) -> Self {
		Self {
			signer: payload.signer,
			genesis_hash: payload.genesis_hash,
			call_data: payload.call_data,
			extensions: payload.extensions,
			tx_ext_version: payload.tx_ext_version,
		}
	}

	/// The payload the signing host reviews and signs. Its contacts were
	/// resolved on the pairing host, so none are left to substitute.
	pub fn into_product_payload(self) -> ProductAccountTxPayload {
		ProductAccountTxPayload {
			signer: self.signer,
			genesis_hash: self.genesis_hash,
			call_data: self.call_data,
			extensions: self.extensions,
			tx_ext_version: self.tx_ext_version,
			contacts: Vec::new(),
		}
	}
}

/// Request sent when a product asks the signing host to create a transaction
/// for a user-imported legacy account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionWithLegacyAccountRequest {
	/// Transaction payload to build.
	pub payload: CreateTransactionLegacyPayload,
}

/// Versioned legacy transaction-creation payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CreateTransactionLegacyPayload {
	/// Version 1 legacy-account payload.
	V1(LegacyAccountTxPayload),
}

/// SCALE-encoded transaction for a product or legacy account, or an error description.
/// Signed unless the request supplied its own V5 `VerifyMultiSignature` extension.
pub type CreateTransactionResponse = Result<Vec<u8>, String>;

/// Decoded inbound statement-channel outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoSessionStatement {
	/// The outbound request statement was acknowledged with a success code.
	RequestAccepted,
	/// Application messages in wire order, each decoded on its own so a
	/// consumer can stop at its match before a later undecodable message.
	RemoteMessages(Vec<Result<v1::RemoteMessage, String>>),
}

/// Decode and classify an inbound encrypted SSO session statement.
pub fn decode_sso_session_statement(
	session: &SsoSessionInfo,
	statement: &[u8],
	expected_statement_request_id: &str,
) -> Result<Option<SsoSessionStatement>, String> {
	let verified =
		decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
	// Freshness gate against replay: a statement whose expiry is in the past
	// is ignored. Trusts the local clock.
	if verified
		.expiry
		.is_some_and(|expiry| statement_expiry_elapsed(expiry, current_unix_secs()))
	{
		return Ok(None);
	}
	let encrypted = verified.data;
	let data = decrypt_session_statement_data(session, &encrypted)?;
	if verified.signer == session.ss_public_key {
		return match data {
			SsoStatementData::Response { request_id, response_code }
				if request_id == expected_statement_request_id =>
			{
				classify_response_ack(request_id, response_code).map(Some)
			},
			_ => Ok(None),
		};
	}
	if verified.signer != session.identity_account_id {
		return Err("statement proof signer does not match expected peer".to_string());
	}
	match data {
		SsoStatementData::Response { request_id, response_code }
			if request_id == expected_statement_request_id =>
		{
			classify_response_ack(request_id, response_code).map(Some)
		},
		SsoStatementData::Response { .. } => Ok(None),
		SsoStatementData::Request { data, .. } => Ok(Some(SsoSessionStatement::RemoteMessages(
			data.iter()
				.map(|message| {
					decode_remote_message(message).map(|message| {
						let RemoteMessageData::V1(message) = message.data;
						message
					})
				})
				.collect(),
		))),
	}
}

fn classify_response_ack(
	request_id: String,
	response_code: u8,
) -> Result<SsoSessionStatement, String> {
	match SsoResponseCode::try_from(response_code) {
		Ok(SsoResponseCode::Success) => Ok(SsoSessionStatement::RequestAccepted),
		Ok(code) => Err(format!("SSO request {request_id} was rejected: {code}")),
		Err(()) => Err(format!("SSO request {request_id} was rejected: unknown")),
	}
}

/// Inbound request decoded from a peer-signed session statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingSsoRequest {
	/// Statement-level request id used by the transport acknowledgement.
	pub request_id: String,
	/// Statement expiry as unix seconds, when supplied by the peer.
	pub expires_at_unix_secs: Option<u64>,
	/// Application messages batched into the request.
	pub messages: Vec<RemoteMessage>,
}

/// Failure decoding a peer request statement.
///
/// `request_id` is present when the encrypted envelope decoded far enough for
/// the responder to acknowledge the statement with
/// [`SsoResponseCode::DecodingFailed`].
#[derive(Debug, Clone, derive_more::Display)]
#[display("{reason}")]
pub struct SsoRequestDecodeError {
	/// Statement-level request id, when recoverable.
	pub request_id: Option<String>,
	/// Human-readable failure description.
	pub reason: String,
}

impl SsoRequestDecodeError {
	fn unrecoverable(reason: String) -> Self {
		Self { request_id: None, reason }
	}
}

/// Decode a peer request for a signing-host responder.
///
/// Own echoes, response acknowledgements, and expired statements are ignored.
pub fn decode_incoming_sso_request(
	session: &SsoSessionInfo,
	statement: &[u8],
) -> Result<Option<IncomingSsoRequest>, SsoRequestDecodeError> {
	let verified = decode_verified_statement_data(statement, None)
		.map_err(|err| SsoRequestDecodeError::unrecoverable(err.to_string()))?;
	if verified.signer == session.ss_public_key {
		return Ok(None);
	}
	if verified.signer != session.identity_account_id {
		return Err(SsoRequestDecodeError::unrecoverable(
			"statement proof signer does not match expected peer".to_string(),
		));
	}
	if verified
		.expiry
		.is_some_and(|expiry| statement_expiry_elapsed(expiry, current_unix_secs()))
	{
		return Ok(None);
	}
	match decrypt_session_statement_data(session, &verified.data)
		.map_err(SsoRequestDecodeError::unrecoverable)?
	{
		SsoStatementData::Response { .. } => Ok(None),
		SsoStatementData::Request { request_id, data } => {
			let messages = data
				.iter()
				.map(|message| decode_remote_message(message))
				.collect::<Result<Vec<_>, _>>()
				.map_err(|reason| SsoRequestDecodeError {
					request_id: Some(request_id.clone()),
					reason,
				})?;
			Ok(Some(IncomingSsoRequest {
				request_id,
				expires_at_unix_secs: verified.expiry.map(|expiry| expiry >> 32),
				messages,
			}))
		},
	}
}

pub fn decode_remote_message(message: &[u8]) -> Result<RemoteMessage, String> {
	let mut input = message;
	let decoded = RemoteMessage::decode(&mut input)
		.map_err(|error| format!("invalid SSO remote message: {error}"))?;
	if !input.is_empty() {
		return Err("invalid SSO remote message: trailing bytes".to_string());
	}
	Ok(decoded)
}

/// Build the signed transport acknowledgement for a peer-initiated request.
pub fn build_signed_session_response_statement(
	session: &SsoSessionInfo,
	request_id: String,
	response_code: u8,
	expiry: u64,
) -> Result<Vec<u8>, String> {
	let encrypted = encrypt_session_statement_data(
		session,
		&SsoStatementData::Response { request_id, response_code },
	)?;
	build_signed_statement(
		session,
		peer_response_channel(session),
		session.session_id_peer,
		encrypted,
		expiry,
	)
}

/// Build a signed outbound SSO request statement with a random nonce.
pub fn build_outgoing_request_statement(
	session: &SsoSessionInfo,
	statement_request_id: String,
	messages: Vec<RemoteMessage>,
	expiry: u64,
) -> Result<Vec<u8>, String> {
	let encrypted = encrypt_outgoing_request_data(session, statement_request_id, messages)?;
	build_signed_session_request_statement(session, encrypted, expiry)
}

/// Build a signed outbound SSO request statement with a caller-supplied nonce.
#[cfg(test)]
pub fn build_outgoing_request_statement_with_nonce(
	session: &SsoSessionInfo,
	statement_request_id: String,
	messages: Vec<RemoteMessage>,
	expiry: u64,
	nonce: [u8; AEAD_NONCE_LEN],
) -> Result<Vec<u8>, String> {
	let encrypted =
		encrypt_outgoing_request_data_with_nonce(session, statement_request_id, messages, nonce)?;
	build_signed_session_request_statement(session, encrypted, expiry)
}

fn encrypt_outgoing_request_data(
	session: &SsoSessionInfo,
	statement_request_id: String,
	messages: Vec<RemoteMessage>,
) -> Result<Vec<u8>, String> {
	encrypt_session_statement_data(session, &outgoing_request_data(statement_request_id, messages))
}

#[cfg(test)]
fn encrypt_outgoing_request_data_with_nonce(
	session: &SsoSessionInfo,
	statement_request_id: String,
	messages: Vec<RemoteMessage>,
	nonce: [u8; AEAD_NONCE_LEN],
) -> Result<Vec<u8>, String> {
	encrypt_session_statement_data_with_nonce(
		session,
		&outgoing_request_data(statement_request_id, messages),
		nonce,
	)
}

fn outgoing_request_data(
	statement_request_id: String,
	messages: Vec<RemoteMessage>,
) -> SsoStatementData {
	SsoStatementData::Request {
		request_id: statement_request_id,
		data: messages.into_iter().map(|message| message.encode()).collect(),
	}
}

#[cfg(test)]
mod tests;
