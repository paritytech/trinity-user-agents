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
use zeroize::Zeroizing;

use parity_scale_codec::{Decode, Encode};
use truapi::latest::{
    AccountId, AllocatableResource, HostAccountCreateProofResponse, HostAccountGetAliasResponse,
    HostAccountSignVrfError, HostSignPayloadRequest, HostSignPayloadResponse, HostSignRawRequest,
    LegacyAccountTxPayload, ProductAccountTxPayload, RawPayload, RegisteredRingVrfKey,
    VrfSignature,
};
use truapi::v01;

use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::pairing::{
    AEAD_NONCE_LEN, SsoStatementData, decrypt_session_statement_data,
    encrypt_session_statement_data, encrypt_session_statement_data_with_nonce,
    peer_response_channel,
};
use crate::host_logic::statement_store::{
    build_signed_session_request_statement, build_signed_statement, current_unix_secs,
    decode_verified_statement_data, statement_expiry_elapsed,
};

pub mod v1;

/// Transport-level acknowledgement code for an SSO session statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, derive_more::Display)]
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

/// Outcome of answering one SSO remote message on behalf of a caller that
/// owns the session transport. Generic over the response representation:
/// the typed runtime layer carries a decoded [`RemoteMessage`], the FFI
/// boundary carries its SCALE encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoRequestOutcome<T> {
    /// Response to post back over the session.
    Response(T),
    /// The peer ended the session; the caller tears down its transport and
    /// records. The core holds no per-peer state to clear.
    Disconnected,
    /// Not a request (a `*Response` variant); nothing to do.
    Ignored,
}

/// A product's canonical request payload and the identity of its caller.
///
/// SCALE encodes the caller followed directly by the payload fields.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductRequest<P> {
    /// Product making the request, as attested by the paired host.
    ///
    /// The signing host cannot observe the paired host's products, so it keys
    /// permissions and product-derived keys on this attested identity.
    pub calling_product_id: String,
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
    pub account: AccountId,
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
    pub calling_product_id: String,
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
    /// Product account was registered as the current Statement Store slot target.
    ProductStatementStoreAllowance,
}

impl SsoAllocatedResource {
    /// Stable, non-secret resource discriminant suitable for errors and logs.
    pub(crate) const fn kind(&self) -> &'static str {
        match self {
            Self::StatementStoreAllowance { .. } => "statement-store-allowance",
            Self::BulletinAllowance { .. } => "bulletin-allowance",
            Self::SmartContractAllowance => "smart-contract-allowance",
            Self::AutoSigning { .. } => "auto-signing",
            Self::ProductStatementStoreAllowance => "product-statement-store-allowance",
        }
    }
}

impl fmt::Debug for SsoAllocatedResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind())
    }
}

/// Consent-free request for `//product//{product_id}`'s sr25519 public key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductSubtreeRequest {
    /// DotNS product identifier whose hard subtree is requested.
    pub product_id: String,
}

/// Account Holder response carrying a product subtree public key.
pub type ProductSubtreeResponse = Result<[u8; 32], String>;
/// Exact unsigned Statement Store payload to sign with a product-derived account.
///
/// The signing host validates the payload as canonical unsigned statement
/// fields before signing, so this cannot become a generic signing oracle.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StatementStoreProductSignRequest {
    /// Product making the request.
    pub calling_product_id: String,
    /// Product account that signs the statement payload.
    pub account: v01::ProductAccountId,
    /// Exact unsigned statement fields, without their SCALE vector prefix.
    pub payload: Vec<u8>,
}

/// Account Holder response carrying the product-account sr25519 signature.
pub type StatementStoreProductSignResponse = Result<[u8; 64], String>;

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
    V1(ProductAccountTxPayload),
}

/// Request sent when a product asks the signing host to create a transaction
/// for a user-imported legacy account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CreateTransactionWithLegacyAccountRequest {
    /// Transaction payload to build.
    pub payload: CreateTransactionLegacyPayload,
}

/// Host-owned Chat operations carried over encrypted SSO.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SsoProductDeviceChatOperation {
    /// Retained for decoding only; high-level Chat execution is retired.
    #[codec(index = 6)]
    V2(truapi::v02::HostProductDeviceChatRequest),
    /// Narrow product-owned Chat boundary; tags 0 through 5 remain invalid.
    #[codec(index = 7)]
    V3(truapi::latest::HostProductDeviceChatRequest),
}

/// Public Chat views and typed failures returned by the signing authority.
pub type ProductDeviceChatResponse = Result<
    truapi::versioned::account::HostProductDeviceChatResponse,
    truapi::versioned::account::HostProductDeviceChatError,
>;

/// Incoming payment funding; source secrets stay inside the encrypted SSO channel.
///
/// The wrapper also guards requests while queued or cancelled before dispatch.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct PaymentTopUpRequest {
    /// Caller authenticated by the product connection or paired host.
    pub calling_product_id: String,
    /// Existing versioned payment request, including caller-supplied funding keys.
    pub payload: truapi::versioned::payment::HostPaymentTopUpRequest,
}

impl PaymentTopUpRequest {
    pub(crate) fn into_parts(mut self) -> (String, truapi::v01::HostPaymentTopUpRequest) {
        let truapi::versioned::payment::HostPaymentTopUpRequest::V1(payload) = &mut self.payload;
        let payload = core::mem::replace(
            payload,
            truapi::v01::HostPaymentTopUpRequest {
                into: None,
                amount: 0,
                source: truapi::v01::PaymentTopUpSource::Coins {
                    sr25519_secret_keys: Vec::new(),
                },
            },
        );
        (core::mem::take(&mut self.calling_product_id), payload)
    }
}

impl fmt::Debug for PaymentTopUpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaymentTopUpRequest")
            .field("calling_product_id", &self.calling_product_id)
            .field("payload", &"[REDACTED]")
            .finish()
    }
}

impl Drop for PaymentTopUpRequest {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        let truapi::versioned::payment::HostPaymentTopUpRequest::V1(payload) = &mut self.payload;
        match &mut payload.source {
            truapi::v01::PaymentTopUpSource::PrivateKey { sr25519_secret_key } => {
                sr25519_secret_key.zeroize();
            }
            truapi::v01::PaymentTopUpSource::Coins {
                sr25519_secret_keys,
            } => {
                sr25519_secret_keys.zeroize();
            }
            truapi::v01::PaymentTopUpSource::ProductAccount { .. } => {}
        }
    }
}

/// Payment failure with a secret-safe transcript representation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PaymentTopUpError(pub truapi::v01::HostPaymentTopUpError);

impl fmt::Display for PaymentTopUpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match &self.0 {
            truapi::v01::HostPaymentTopUpError::InsufficientFunds => "Insufficient funds",
            truapi::v01::HostPaymentTopUpError::InvalidSource => "Invalid payment source",
            truapi::v01::HostPaymentTopUpError::PartialPayment { .. } => "Partial payment",
            truapi::v01::HostPaymentTopUpError::Unknown { .. } => "Payment top-up unavailable",
        })
    }
}

/// Success is returned only after the wallet has credited the incoming funding.
pub type PaymentTopUpResponse = Result<(), PaymentTopUpError>;

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
    let data = Zeroizing::new(decrypt_session_statement_data(session, &encrypted)?);
    if verified.signer == session.ss_public_key {
        return match &*data {
            SsoStatementData::Response {
                request_id,
                response_code,
            } if request_id == expected_statement_request_id => {
                classify_response_ack(request_id.clone(), *response_code).map(Some)
            }
            _ => Ok(None),
        };
    }
    if verified.signer != session.identity_account_id {
        return Err("statement proof signer does not match expected peer".to_string());
    }
    match &*data {
        SsoStatementData::Response {
            request_id,
            response_code,
        } if request_id == expected_statement_request_id => {
            classify_response_ack(request_id.clone(), *response_code).map(Some)
        }
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
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("{reason}")]
pub struct SsoRequestDecodeError {
    /// Statement-level request id, when recoverable.
    pub request_id: Option<String>,
    /// Human-readable failure description.
    pub reason: String,
}

impl SsoRequestDecodeError {
    fn unrecoverable(reason: String) -> Self {
        Self {
            request_id: None,
            reason,
        }
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
    let data = Zeroizing::new(
        decrypt_session_statement_data(session, &verified.data)
            .map_err(SsoRequestDecodeError::unrecoverable)?,
    );
    match &*data {
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
                request_id: request_id.clone(),
                expires_at_unix_secs: verified.expiry.map(|expiry| expiry >> 32),
                messages,
            }))
        }
    }
}

pub(crate) fn decode_remote_message(message: &[u8]) -> Result<RemoteMessage, String> {
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
        &SsoStatementData::Response {
            request_id,
            response_code,
        },
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
    encrypt_session_statement_data(
        session,
        &Zeroizing::new(outgoing_request_data(statement_request_id, messages)),
    )
}

fn encrypt_outgoing_request_data_with_nonce(
    session: &SsoSessionInfo,
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
    nonce: [u8; AEAD_NONCE_LEN],
) -> Result<Vec<u8>, String> {
    encrypt_session_statement_data_with_nonce(
        session,
        &Zeroizing::new(outgoing_request_data(statement_request_id, messages)),
        nonce,
    )
}

fn outgoing_request_data(
    statement_request_id: String,
    messages: Vec<RemoteMessage>,
) -> SsoStatementData {
    SsoStatementData::Request {
        request_id: statement_request_id,
        data: messages
            .into_iter()
            .map(|message| message.encode())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::sso::pairing::decrypt_session_statement_data;
    use crate::host_logic::sso::wire::SsoRequest;
    use crate::host_logic::statement_store::{
        StatementField, build_signed_statement, decode_statement_data,
        unsigned_statement_signing_payload,
    };
    use crate::test_support::sso_host_and_responder_sessions;
    use schnorrkel::{ExpansionMode, MiniSecretKey};
    use truapi::latest::{
        DerivationIndex, HostAccountSignVrfRequest, ProductAccountId, ProductProofContext,
        RingLocation,
    };
    use truapi::latest::{HostSignPayloadData, TxPayloadExtension};
    use truapi::v01::RingLocationJunction;
    use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519SecretKey};

    fn account() -> ProductAccountId {
        ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: DerivationIndex::Index(7),
        }
    }

    fn fresh_expiry() -> u64 {
        (current_unix_secs() + 60) << 32
    }

    fn elapsed_expiry() -> u64 {
        (current_unix_secs() - 60) << 32
    }

    fn session() -> SsoSessionInfo {
        let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
        let core_secret = X25519SecretKey::from([1; 32]);
        let peer_secret = X25519SecretKey::from([2; 32]);
        SsoSessionInfo {
            ss_secret: keypair.secret.to_bytes(),
            ss_public_key: keypair.public.to_bytes(),
            enc_secret: core_secret.to_bytes(),
            peer_enc_pubkey: X25519PublicKey::from(&peer_secret).to_bytes(),
            identity_account_id: [3; 32],
            session_id_own: [4; 32],
            session_id_peer: [5; 32],
            request_channel: [6; 32],
            response_channel: [7; 32],
            peer_request_channel: [8; 32],
        }
    }

    #[test]
    fn disconnected_message_matches_host_papp_variant_order() {
        let message = RemoteMessage {
            message_id: String::new(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };

        assert_eq!(message.encode(), vec![0, 0, 0]);
        assert_eq!(message.name(), "Disconnected");
    }

    #[test]
    fn raw_sign_request_uses_remote_message_variant_indices() {
        let message = RemoteMessage::request(
            "m1".to_string(),
            SignRequest::Raw(truapi::latest::HostSignRawRequest {
                account: account(),
                payload: RawPayload::Bytes {
                    bytes: vec![0xde, 0xad],
                },
            }),
        );
        let encoded = message.encode();

        assert_eq!(&encoded[..3], &[8, b'm', b'1']);
        assert_eq!(encoded[3], 0);
        assert_eq!(encoded[4], 1);
        assert_eq!(encoded[5], 1);
    }

    #[test]
    fn late_remote_message_variants_match_host_papp_order() {
        let ring_location = RingLocation {
            chain_id: [0; 32],
            junctions: vec![],
        };
        let key_handle = ProductAccountId {
            dot_ns_identifier: "peopl.dot".to_string(),
            derivation_index: DerivationIndex::Index(0),
        };
        let legacy_tx = RemoteMessage::request(
            String::new(),
            CreateTransactionWithLegacyAccountRequest {
                payload: CreateTransactionLegacyPayload::V1(LegacyAccountTxPayload {
                    signer: [1; 32],
                    genesis_hash: [2; 32],
                    call_data: Vec::new(),
                    extensions: Vec::new(),
                    tx_ext_version: 0,
                }),
            },
        )
        .encode();
        let legacy_raw = RemoteMessage::request(
            String::new(),
            SignRawWithLegacyAccountRequest {
                account: [1; 32],
                data: RawPayload::Bytes { bytes: vec![] },
            },
        )
        .encode();
        let register = RemoteMessage::request(
            String::new(),
            ProductRequest {
                calling_product_id: "caller.dot".to_string(),
                payload: truapi::latest::HostAccountRegisterRingVrfKeyRequest {
                    index: DerivationIndex::Index(0),
                    ring: ring_location.clone(),
                },
            },
        )
        .encode();
        let register_response = RemoteMessage {
            message_id: String::new(),
            data: RemoteMessageData::V1(v1::RemoteMessage::RegisterRingVrfKeyResponse(Response {
                responding_to: String::new(),
                payload: Ok([1; 32]),
            })),
        }
        .encode();
        let list = RemoteMessage::request(
            String::new(),
            ProductRequest {
                calling_product_id: "caller.dot".to_string(),
                payload: truapi::latest::HostAccountListRingVrfKeysRequest {
                    owner: "peopl.dot".to_string(),
                    disclosure: truapi::v01::RingVrfKeyDisclosure::Anonymized,
                },
            },
        )
        .encode();
        let list_response = RemoteMessage {
            message_id: String::new(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ListRingVrfKeysResponse(Response {
                responding_to: String::new(),
                payload: Ok(Vec::new()),
            })),
        }
        .encode();
        let sign = RemoteMessage::request(
            String::new(),
            ProductRequest {
                calling_product_id: "caller.dot".to_string(),
                payload: truapi::latest::HostAccountRingVrfSignRequest {
                    key_handle,
                    message: vec![],
                },
            },
        )
        .encode();
        let sign_response = RemoteMessage {
            message_id: String::new(),
            data: RemoteMessageData::V1(v1::RemoteMessage::RingVrfSignResponse(Response {
                responding_to: String::new(),
                payload: Ok(Vec::new()),
            })),
        }
        .encode();

        assert_eq!(legacy_tx[..3], [0, 0, 9]);
        assert_eq!(legacy_raw[..3], [0, 0, 10]);
        assert_eq!(register[..3], [0, 0, 18]);
        assert_eq!(register_response[..3], [0, 0, 19]);
        assert_eq!(list[..3], [0, 0, 20]);
        assert_eq!(list_response[..3], [0, 0, 21]);
        assert_eq!(sign[..3], [0, 0, 22]);
        assert_eq!(sign_response[..3], [0, 0, 23]);
        assert_eq!(RingVrfError::RingNotFound.encode()[0], 0);
        assert_eq!(RingVrfError::NotMember.encode()[0], 1);
        assert_eq!(RingVrfError::KeyNotRegistered.encode()[0], 2);
        assert_eq!(RingVrfError::KeyNotInRing.encode()[0], 3);
        assert_eq!(RingVrfError::NotAllowlisted.encode()[0], 4);
        assert_eq!(RingVrfError::Rejected.encode()[0], 5);
        assert_eq!(
            RingVrfError::Unknown {
                reason: String::new()
            }
            .encode()[0],
            6
        );
    }

    #[test]
    fn rfc_0024_requests_match_android_scale_fixtures() {
        let ring = RingLocation {
            chain_id: [7; 32],
            junctions: vec![
                RingLocationJunction::PalletInstance(9),
                RingLocationJunction::CollectionId(b"pop:polkadot.network/people     ".to_vec()),
            ],
        };
        let handle = ProductAccountId {
            dot_ns_identifier: "peopl.dot".to_string(),
            derivation_index: DerivationIndex::Index(0),
        };
        let messages = [
            v1::RemoteMessage::RegisterRingVrfKeyRequest(ProductRequest {
                calling_product_id: "game.dot".to_string(),
                payload: truapi::latest::HostAccountRegisterRingVrfKeyRequest {
                    index: DerivationIndex::Index(4),
                    ring: ring.clone(),
                },
            }),
            v1::RemoteMessage::ListRingVrfKeysRequest(ProductRequest {
                calling_product_id: "game.dot".to_string(),
                payload: truapi::latest::HostAccountListRingVrfKeysRequest {
                    owner: "peopl.dot".to_string(),
                    disclosure: truapi::v01::RingVrfKeyDisclosure::PublicKey,
                },
            }),
            v1::RemoteMessage::RingVrfSignRequest(ProductRequest {
                calling_product_id: "game.dot".to_string(),
                payload: truapi::latest::HostAccountRingVrfSignRequest {
                    key_handle: handle,
                    message: (0..16).collect(),
                },
            }),
        ];
        let expected = [
            "0x122067616d652e646f74000400000007070707070707070707070707070707070707070707070707070707070707070800090180706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652020202020",
            "0x142067616d652e646f742470656f706c2e646f7401",
            "0x162067616d652e646f742470656f706c2e646f74000000000040000102030405060708090a0b0c0d0e0f",
        ];
        for (message, expected) in messages.into_iter().zip(expected) {
            assert_eq!(format!("0x{}", hex::encode(message.encode())), expected);
        }
    }

    #[test]
    fn ring_vrf_messages_wire_shape_pin() {
        let context = ProductProofContext {
            product_id: "voting.dot".to_string(),
            suffix: DerivationIndex::Index(0),
        };
        let ring_location = RingLocation {
            chain_id: [0x11; 32],
            junctions: vec![
                RingLocationJunction::PalletInstance(67),
                RingLocationJunction::CollectionId(b"pop".to_vec()),
            ],
        };
        let key_handle = ProductAccountId {
            dot_ns_identifier: "peopl.dot".to_string(),
            derivation_index: DerivationIndex::Index(0),
        };

        let alias = RemoteMessage::request(
            "m-alias".to_string(),
            ProductRequest {
                calling_product_id: "caller.dot".to_string(),
                payload: truapi::latest::HostAccountGetAliasRequest {
                    key_handle: key_handle.clone(),
                    context: context.clone(),
                    ring_location: ring_location.clone(),
                },
            },
        );
        let proof = RemoteMessage::request(
            "m-proof".to_string(),
            ProductRequest {
                calling_product_id: "caller.dot".to_string(),
                payload: truapi::latest::HostAccountCreateProofRequest {
                    key_handle,
                    context,
                    ring_location,
                    message: b"vote".to_vec(),
                },
            },
        );

        assert_host_papp_0_8_11_fixture(
            alias,
            "0x1c6d2d616c69617300032863616c6c65722e646f742470656f706c2e646f74000000000028766f74696e672e646f7400000000001111111111111111111111111111111111111111111111111111111111111111080043010c706f70",
        );
        assert_host_papp_0_8_11_fixture(
            proof,
            "0x1c6d2d70726f6f66000c2863616c6c65722e646f742470656f706c2e646f74000000000028766f74696e672e646f7400000000001111111111111111111111111111111111111111111111111111111111111111080043010c706f7010766f7465",
        );
    }

    #[test]
    fn ring_vrf_response_messages_match_host_papp_0_8_11_fixtures() {
        let contextual_alias = HostAccountGetAliasResponse {
            context: [0x22; 32],
            alias: vec![0x33, 0x44],
        };
        let alias_response = RemoteMessage {
            message_id: "r-alias".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::GetAccountAliasResponse(Response {
                responding_to: "m-alias".to_string(),
                payload: Ok(contextual_alias.clone()),
            })),
        };
        let proof_response = RemoteMessage {
            message_id: "r-proof".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::CreateAccountProofResponse(Response {
                responding_to: "m-proof".to_string(),
                payload: Ok(HostAccountCreateProofResponse {
                    proof: vec![0x55, 0x66],
                    contextual_alias,
                    ring_index: 7,
                    ring_revision: 9,
                }),
            })),
        };

        assert_host_papp_0_8_11_fixture(
            alias_response,
            "0x1c722d616c69617300041c6d2d616c696173002222222222222222222222222222222222222222222222222222222222222222083344",
        );
        assert_host_papp_0_8_11_fixture(
            proof_response,
            "0x1c722d70726f6f66000d1c6d2d70726f6f660008556622222222222222222222222222222222222222222222222222222222222222220833440700000009000000",
        );
    }

    fn sequential_bytes<const N: usize>(start: u8) -> [u8; N] {
        std::array::from_fn(|index| start.wrapping_add(index as u8))
    }

    fn assert_host_papp_0_8_11_fixture(message: RemoteMessage, expected: &str) {
        assert_eq!(
            hex::encode(message.encode()),
            expected.trim_start_matches("0x")
        );
    }

    #[test]
    fn product_subtree_messages_match_mobile_wire_contract() {
        let request = RemoteMessage::request(
            "request".to_string(),
            ProductSubtreeRequest {
                product_id: "browse.dot".to_string(),
            },
        );
        assert_eq!(
            hex::encode(request.encode()),
            "1c7265717565737400102862726f7773652e646f74"
        );

        let response = RemoteMessage {
            message_id: "response".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(Response {
                responding_to: "request".to_string(),
                payload: Ok([0xAB; 32]),
            })),
        };
        assert_eq!(
            hex::encode(response.encode()),
            format!(
                "20726573706f6e736500111c7265717565737400{}",
                "ab".repeat(32)
            )
        );
        let RemoteMessageData::V1(data) = response.data;
        assert_eq!(
            ProductSubtreeRequest::response_from_message(data),
            Some(Response {
                responding_to: "request".to_string(),
                payload: Ok([0xAB; 32]),
            })
        );
    }

    #[test]
    fn product_device_chat_messages_preserve_typed_operations_and_failures() {
        let chat_request = ProductRequest {
            calling_product_id: "egui-chat.paseo".to_string(),
            payload: SsoProductDeviceChatOperation::V3(
                truapi::latest::HostProductDeviceChatRequest::SendPayment {
                    peer_identity: [0x55; 32],
                    request_id: "payment-one".to_string(),
                    amount_cents: 19,
                },
            ),
        };
        let request = RemoteMessage::request("request".to_string(), chat_request);
        let encoded_request = request.encode();
        assert_eq!(encoded_request[9], 24);
        assert_eq!(
            RemoteMessage::decode(&mut encoded_request.as_slice()).unwrap(),
            request
        );

        let product_response: ProductDeviceChatResponse =
            Err(truapi::versioned::account::HostProductDeviceChatError::V1(
                truapi::latest::HostProductDeviceChatError::OperationConflict,
            ));
        let response_envelope = Response {
            responding_to: "request".to_string(),
            payload: product_response,
        };
        let response = RemoteMessage {
            message_id: "response".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ProductDeviceChatResponse(
                response_envelope.clone(),
            )),
        };
        let encoded_response = response.encode();
        assert_eq!(encoded_response[10], 25);
        assert_eq!(
            RemoteMessage::decode(&mut encoded_response.as_slice()).unwrap(),
            response
        );
        let RemoteMessageData::V1(data) = response.data;
        assert_eq!(
            ProductRequest::<SsoProductDeviceChatOperation>::response_from_message(data),
            Some(response_envelope)
        );
    }

    #[test]
    fn product_device_chat_rejects_removed_raw_crypto_tags() {
        for tag in 0..6u8 {
            let mut legacy = vec![tag];
            legacy.extend_from_slice(&[0; 256]);
            assert!(SsoProductDeviceChatOperation::decode(&mut legacy.as_slice()).is_err());
        }
    }

    #[test]
    fn narrow_chat_uses_a_fresh_sso_operation_tag() {
        let retired = SsoProductDeviceChatOperation::V2(
            truapi::v02::HostProductDeviceChatRequest::Initialize,
        );
        let current = SsoProductDeviceChatOperation::V3(
            truapi::latest::HostProductDeviceChatRequest::Initialize,
        );
        assert_eq!(retired.encode(), [6, 0]);
        assert_eq!(current.encode(), [7, 0]);
        assert_eq!(
            SsoProductDeviceChatOperation::decode(&mut [6, 0].as_slice()).unwrap(),
            retired,
        );
        assert_eq!(
            SsoProductDeviceChatOperation::decode(&mut [7, 0].as_slice()).unwrap(),
            current,
        );
    }
    #[test]
    fn statement_store_product_sign_messages_pin_extension_wire_indices() {
        let payload = unsigned_statement_signing_payload(vec![
            StatementField::Data(vec![1, 2, 3]),
            StatementField::Channel([0x44; 32]),
        ])
        .unwrap();
        let request_payload = StatementStoreProductSignRequest {
            calling_product_id: "egui-chat.paseo".to_string(),
            account: ProductAccountId {
                dot_ns_identifier: "egui-chat.paseo".to_string(),
                derivation_index: DerivationIndex::Index(0),
            },
            payload,
        };
        let request = RemoteMessage::request("request".to_string(), request_payload);
        let encoded_request = request.encode();
        assert_eq!(encoded_request[9], 26);
        assert_eq!(
            RemoteMessage::decode(&mut encoded_request.as_slice()).unwrap(),
            request
        );

        let response_envelope = Response {
            responding_to: "request".to_string(),
            payload: Ok([0xAB; 64]),
        };
        let response = RemoteMessage {
            message_id: "response".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::StatementStoreProductSignResponse(
                response_envelope.clone(),
            )),
        };
        let encoded_response = response.encode();
        assert_eq!(encoded_response[10], 27);
        let RemoteMessageData::V1(data) = response.data;
        assert_eq!(
            StatementStoreProductSignRequest::response_from_message(data),
            Some(response_envelope)
        );
    }

    #[test]
    fn payment_top_up_appends_wire_tags_and_retains_response_matching() {
        let request = RemoteMessage::request(
            String::new(),
            PaymentTopUpRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload: truapi::versioned::payment::HostPaymentTopUpRequest::V1(
                    truapi::v01::HostPaymentTopUpRequest {
                        into: None,
                        amount: 1,
                        source: truapi::v01::PaymentTopUpSource::Coins {
                            sr25519_secret_keys: vec![[0xab; 64]],
                        },
                    },
                ),
            },
        );
        let encoded = Zeroizing::new(request.encode());
        assert_eq!(&encoded[..3], &[0, 0, 28]);
        assert_eq!(decode_remote_message(&encoded).unwrap(), request);
        let response = Response {
            responding_to: "top-up".to_string(),
            payload: Err(PaymentTopUpError(
                truapi::v01::HostPaymentTopUpError::PartialPayment { credited: 7 },
            )),
        };
        let response = PaymentTopUpRequest::response_into_message(response);
        assert_eq!(response.encode()[0], 29);
        assert!(ProductSubtreeRequest::response_from_message(response.clone()).is_none());
        assert_eq!(
            PaymentTopUpRequest::response_from_message(response)
                .unwrap()
                .payload,
            Err(PaymentTopUpError(
                truapi::v01::HostPaymentTopUpError::PartialPayment { credited: 7 }
            )),
        );
        let older_response = ProductSubtreeRequest::response_into_message(Response {
            responding_to: "subtree".to_string(),
            payload: Ok([7; 32]),
        });
        assert!(PaymentTopUpRequest::response_from_message(older_response.clone()).is_none());
        assert_eq!(
            ProductSubtreeRequest::response_from_message(older_response)
                .unwrap()
                .payload,
            Ok([7; 32]),
        );
    }

    #[test]
    fn sign_vrf_messages_match_mobile_wire_contract() {
        let payload = HostAccountSignVrfRequest {
            account: ProductAccountId {
                dot_ns_identifier: "browse.dot".to_string(),
                derivation_index: DerivationIndex::Index(7),
            },
            transcript_label: b"ctx".to_vec(),
            items: vec![truapi::v01::VrfTranscriptItem {
                label: b"domain".to_vec(),
                value: vec![1, 2],
            }],
        };
        let request = RemoteMessage::request(
            "req".to_string(),
            ProductRequest {
                calling_product_id: "browse.dot".to_string(),
                payload,
            },
        );
        assert_eq!(
            hex::encode(request.encode()),
            "0c726571000e2862726f7773652e646f742862726f7773652e646f7400070000000c6374780418646f6d61696e080102"
        );

        let response = RemoteMessage {
            message_id: "resp".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::SignVrfResponse(Response {
                responding_to: "req".to_string(),
                payload: Ok(VrfSignature {
                    pre_output: [0x11; 32],
                    proof: [0x22; 64],
                }),
            })),
        };
        assert_eq!(
            hex::encode(response.encode()),
            format!(
                "1072657370000f0c72657100{}{}",
                "11".repeat(32),
                "22".repeat(64)
            )
        );
        let RemoteMessageData::V1(data) = response.data;
        assert!(matches!(
            ProductRequest::<truapi::latest::HostAccountSignVrfRequest>::response_from_message(
                data
            ),
            Some(Response {
                payload: Ok(VrfSignature { .. }),
                ..
            })
        ));
    }

    #[test]
    fn auto_signing_secret_is_fixed_width_on_the_mobile_wire() {
        let message = RemoteMessage {
            message_id: "m".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(Response {
                responding_to: "r".to_string(),
                payload: Ok(vec![SsoAllocationOutcome::Allocated(
                    SsoAllocatedResource::AutoSigning {
                        product_root_private_key: sequential_bytes(0),
                        ring_vrf_domain_entropy: sequential_bytes(64),
                    },
                )]),
            })),
        };
        assert_eq!(
            hex::encode(message.encode()),
            format!(
                "046d0006047200040003{}{}",
                hex::encode(sequential_bytes::<64>(0)),
                hex::encode(sequential_bytes::<32>(64))
            )
        );
    }

    #[test]
    fn remote_message_decoder_rejects_trailing_bytes() {
        let mut encoded = RemoteMessage {
            message_id: "m".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        }
        .encode();
        encoded.push(0);

        assert_eq!(
            decode_remote_message(&encoded),
            Err("invalid SSO remote message: trailing bytes".to_string())
        );
    }

    #[test]
    fn allocated_resource_debug_redacts_private_material_through_all_wrappers() {
        let allowance = SsoAllocatedResource::StatementStoreAllowance {
            slot_account_key: vec![222, 173, 190, 239],
        };
        let allowance_debug = format!("{allowance:?}");
        assert_eq!(allowance_debug, "statement-store-allowance");
        assert!(!allowance_debug.contains("222, 173, 190, 239"));
        assert_eq!(
            allowance.encode(),
            vec![0, 16, 222, 173, 190, 239],
            "custom Debug must not alter the SCALE resource layout",
        );

        let bulletin = SsoAllocatedResource::BulletinAllowance {
            slot_account_key: vec![202, 254, 186, 190],
        };
        let bulletin_debug = format!("{bulletin:?}");
        assert_eq!(bulletin_debug, "bulletin-allowance");
        assert!(!bulletin_debug.contains("202, 254, 186, 190"));
        assert_eq!(
            bulletin.encode(),
            vec![1, 16, 202, 254, 186, 190],
            "custom Debug must not alter the SCALE resource layout",
        );

        let auto_signing_secret = [0xA5; 64];
        let response = Response {
            responding_to: "secret-test".to_string(),
            payload: Ok(vec![SsoAllocationOutcome::Allocated(
                SsoAllocatedResource::AutoSigning {
                    product_root_private_key: auto_signing_secret,
                    ring_vrf_domain_entropy: [0x5A; 32],
                },
            )]),
        };
        let response_debug = format!("{response:?}");
        assert!(response_debug.contains("auto-signing"));
        assert!(!response_debug.contains("165, 165"));

        let session_statement = SsoSessionStatement::RemoteMessages(vec![Ok(
            v1::RemoteMessage::ResourceAllocationResponse(response.clone()),
        )]);
        let statement_debug = format!("{session_statement:?}");
        assert!(statement_debug.contains("ResourceAllocation"));
        assert!(statement_debug.contains("auto-signing"));
        assert!(!statement_debug.contains("165, 165"));

        let remote_message = RemoteMessage {
            message_id: "secret-test".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)),
        };
        let message_debug = format!("{remote_message:?}");
        assert!(message_debug.contains("auto-signing"));
        assert!(!message_debug.contains("165, 165"));

        assert!(
            remote_message
                .encode()
                .windows(auto_signing_secret.len())
                .any(|bytes| bytes == &auto_signing_secret[..])
        );
    }

    #[test]
    fn resource_allocation_message_wire_shape_pin() {
        let message = RemoteMessage::request(
            "m-resource".to_string(),
            ResourceAllocationRequest {
                calling_product_id: "truapi-playground.dot".to_string(),
                resources: vec![
                    AllocatableResource::StatementStoreAllowance,
                    AllocatableResource::BulletinAllowance,
                    AllocatableResource::SmartContractAllowance(DerivationIndex::Index(9)),
                    AllocatableResource::AutoSigning,
                ],
                on_existing: OnExistingAllowancePolicy::Increase,
            },
        );

        assert_host_papp_0_8_11_fixture(
            message,
            "0x286d2d7265736f757263650005547472756170692d706c617967726f756e642e646f741000010200090000000301",
        );
    }

    #[test]
    fn create_transaction_message_wire_shape_pin() {
        let message = RemoteMessage::request(
            "m-product-tx".to_string(),
            CreateTransactionRequest {
                payload: CreateTransactionPayload::V1(ProductAccountTxPayload {
                    signer: ProductAccountId {
                        dot_ns_identifier: "truapi-playground.dot".to_string(),
                        derivation_index: DerivationIndex::Index(0),
                    },
                    genesis_hash: sequential_bytes(32),
                    call_data: vec![0, 0],
                    extensions: vec![TxPayloadExtension {
                        id: "CheckNonce".to_string(),
                        extra: vec![1],
                        additional_signed: vec![2, 3],
                    }],
                    tx_ext_version: 0,
                }),
            },
        );

        assert_host_papp_0_8_11_fixture(
            message,
            "0x306d2d70726f647563742d7478000700547472756170692d706c617967726f756e642e646f740000000000202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f0800000428436865636b4e6f6e6365040108020300",
        );
    }

    #[test]
    fn playground_create_transaction_message_wire_shape_pin() {
        let message = RemoteMessage::request(
            "create-transaction-1".to_string(),
            CreateTransactionRequest {
                payload: CreateTransactionPayload::V1(ProductAccountTxPayload {
                    signer: ProductAccountId {
                        dot_ns_identifier: "truapi-playground.dot".to_string(),
                        derivation_index: DerivationIndex::Index(0),
                    },
                    genesis_hash: [
                        0xbf, 0x04, 0x88, 0xdb, 0xe9, 0xda, 0xa1, 0xde, 0x1c, 0x08, 0xc5, 0xf7,
                        0x43, 0xe2, 0x6f, 0xdc, 0x2a, 0x4e, 0xcd, 0x74, 0xcf, 0x87, 0xdd, 0x1b,
                        0x4b, 0x1e, 0xeb, 0x99, 0xae, 0x4e, 0xf1, 0x9f,
                    ],
                    call_data: vec![0, 0],
                    extensions: vec![],
                    tx_ext_version: 0,
                }),
            },
        );

        assert_host_papp_0_8_11_fixture(
            message,
            "0x506372656174652d7472616e73616374696f6e2d31000700547472756170692d706c617967726f756e642e646f740000000000bf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f0800000000",
        );
    }

    #[test]
    fn create_transaction_legacy_message_matches_host_papp_0_8_11_fixture() {
        let message = RemoteMessage::request(
            "m-legacy-tx".to_string(),
            CreateTransactionWithLegacyAccountRequest {
                payload: CreateTransactionLegacyPayload::V1(LegacyAccountTxPayload {
                    signer: sequential_bytes(0),
                    genesis_hash: sequential_bytes(32),
                    call_data: vec![0, 0],
                    extensions: vec![TxPayloadExtension {
                        id: "CheckNonce".to_string(),
                        extra: vec![1],
                        additional_signed: vec![2, 3],
                    }],
                    tx_ext_version: 0,
                }),
            },
        );

        assert_host_papp_0_8_11_fixture(
            message,
            "0x2c6d2d6c65676163792d7478000900000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f0800000428436865636b4e6f6e6365040108020300",
        );
    }

    #[test]
    fn sign_raw_legacy_messages_match_host_papp_0_8_11_fixtures() {
        assert_host_papp_0_8_11_fixture(
            RemoteMessage::request(
                "m-legacy-raw".to_string(),
                SignRawWithLegacyAccountRequest {
                    account: sequential_bytes(0),
                    data: RawPayload::Bytes {
                        bytes: b"Hi".to_vec(),
                    },
                },
            ),
            "0x306d2d6c65676163792d726177000a000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f00084869",
        );
        assert_host_papp_0_8_11_fixture(
            RemoteMessage::request(
                "m-legacy-raw-payload".to_string(),
                SignRawWithLegacyAccountRequest {
                    account: sequential_bytes(0),
                    data: RawPayload::Payload {
                        payload: "<Bytes>Hi</Bytes>".to_string(),
                    },
                },
            ),
            "0x506d2d6c65676163792d7261772d7061796c6f6164000a000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f01443c42797465733e48693c2f42797465733e",
        );
    }

    #[test]
    fn option_bool_matches_host_papp_option_bool_encoding() {
        let mut request = truapi::latest::HostSignPayloadRequest {
            account: account(),
            payload: HostSignPayloadData {
                block_hash: vec![],
                block_number: vec![],
                era: vec![],
                genesis_hash: vec![],
                method: vec![],
                nonce: vec![],
                spec_version: vec![],
                tip: vec![],
                transaction_version: vec![],
                signed_extensions: vec![],
                version: 4,
                asset_id: None,
                metadata_hash: None,
                mode: None,
                with_signed_transaction: parity_scale_codec::OptionBool(Some(true)),
            },
        };
        for (flag, byte) in [(None, 0), (Some(true), 1), (Some(false), 2)] {
            request.payload.with_signed_transaction = parity_scale_codec::OptionBool(flag);
            let expected = hex::decode(format!(
                "246d796170702e646f7400070000000000000000000000000004000000000000{byte:02x}"
            ))
            .unwrap();
            assert_eq!(request.encode(), expected);
            assert_eq!(
                HostSignPayloadRequest::decode(&mut expected.as_slice()).unwrap(),
                request
            );
        }
    }

    #[test]
    fn builds_signed_encrypted_outgoing_request_statement() {
        let session = session();
        let remote_message = RemoteMessage::request(
            "remote-1".to_string(),
            SignRequest::Raw(truapi::latest::HostSignRawRequest {
                account: account(),
                payload: RawPayload::Payload {
                    payload: "<Bytes>hello</Bytes>".to_string(),
                },
            }),
        );

        let statement = build_outgoing_request_statement_with_nonce(
            &session,
            "statement-1".to_string(),
            vec![remote_message.clone()],
            99,
            [9; AEAD_NONCE_LEN],
        )
        .unwrap();
        let encrypted = decode_statement_data(&statement).unwrap();
        let decrypted = decrypt_session_statement_data(&session, &encrypted).unwrap();

        let SsoStatementData::Request { request_id, data } = decrypted else {
            panic!("expected request statement data");
        };
        assert_eq!(request_id, "statement-1");
        assert_eq!(data.len(), 1);
        assert_eq!(
            RemoteMessage::decode(&mut data[0].as_slice()).unwrap(),
            remote_message
        );

        let fields = Vec::<StatementField>::decode(&mut statement.as_slice()).unwrap();
        assert_eq!(fields[1], StatementField::Expiry(99));
        assert_eq!(fields[2], StatementField::Channel(session.request_channel));
        assert_eq!(fields[3], StatementField::Topic1(session.session_id_own));
    }

    #[test]
    fn ignores_own_echoed_session_request_statement() {
        let session = session();
        let remote_message = RemoteMessage::request(
            "remote-1".to_string(),
            SignRequest::Raw(truapi::latest::HostSignRawRequest {
                account: account(),
                payload: RawPayload::Payload {
                    payload: "<Bytes>hello</Bytes>".to_string(),
                },
            }),
        );
        let statement = build_outgoing_request_statement_with_nonce(
            &session,
            "statement-1".to_string(),
            vec![remote_message],
            fresh_expiry(),
            [9; AEAD_NONCE_LEN],
        )
        .unwrap();

        let decoded = decode_sso_session_statement(&session, &statement, "statement-1").unwrap();

        assert_eq!(decoded, None);
    }

    /// A host-built request statement decodes on the responder side into the
    /// batched remote messages, and the responder's ack plus response
    /// statements resolve the host's pending wait.
    #[test]
    fn host_request_round_trips_through_responder_statements() {
        let (host_session, responder_session) = sso_host_and_responder_sessions();
        let request = RemoteMessage::request(
            "remote-1".to_string(),
            SignRequest::Raw(truapi::latest::HostSignRawRequest {
                account: account(),
                payload: RawPayload::Payload {
                    payload: "<Bytes>hello</Bytes>".to_string(),
                },
            }),
        );
        let expiry = fresh_expiry();
        let host_statement = build_outgoing_request_statement(
            &host_session,
            "statement-1".to_string(),
            vec![request.clone()],
            expiry,
        )
        .unwrap();

        let incoming = decode_incoming_sso_request(&responder_session, &host_statement)
            .unwrap()
            .expect("responder should surface the host request");
        assert_eq!(
            incoming,
            IncomingSsoRequest {
                request_id: "statement-1".to_string(),
                expires_at_unix_secs: Some(expiry >> 32),
                messages: vec![request],
            }
        );

        let ack = build_signed_session_response_statement(
            &responder_session,
            incoming.request_id.clone(),
            0,
            fresh_expiry(),
        )
        .unwrap();
        assert_eq!(
            decode_sso_session_statement(&host_session, &ack, "statement-1").unwrap(),
            Some(SsoSessionStatement::RequestAccepted)
        );

        let response = RemoteMessage {
            message_id: "resp-1".to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::SignResponse(Response {
                responding_to: "remote-1".to_string(),
                payload: Ok(HostSignPayloadResponse {
                    signature: vec![9; 64],
                    signed_transaction: None,
                }),
            })),
        };
        let response_statement = build_outgoing_request_statement(
            &responder_session,
            "resp-statement-1".to_string(),
            vec![response],
            fresh_expiry(),
        )
        .unwrap();
        let decoded =
            decode_sso_session_statement(&host_session, &response_statement, "statement-1")
                .unwrap();
        assert_eq!(
            decoded,
            Some(SsoSessionStatement::RemoteMessages(vec![Ok(
                v1::RemoteMessage::SignResponse(Response {
                    responding_to: "remote-1".to_string(),
                    payload: Ok(HostSignPayloadResponse {
                        signature: vec![9; 64],
                        signed_transaction: None,
                    }),
                })
            )]))
        );
    }

    #[test]
    fn responder_ignores_own_echo_and_transport_acks() {
        let (host_session, responder_session) = sso_host_and_responder_sessions();
        let own_statement = build_outgoing_request_statement(
            &responder_session,
            "resp-statement-1".to_string(),
            vec![RemoteMessage {
                message_id: "resp-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
            }],
            fresh_expiry(),
        )
        .unwrap();
        assert_eq!(
            decode_incoming_sso_request(&responder_session, &own_statement).unwrap(),
            None
        );

        let host_ack = build_signed_session_response_statement(
            &host_session,
            "resp-statement-1".to_string(),
            0,
            fresh_expiry(),
        )
        .unwrap();
        assert_eq!(
            decode_incoming_sso_request(&responder_session, &host_ack).unwrap(),
            None
        );
    }

    #[test]
    fn responder_ignores_expired_host_request() {
        let (host_session, responder_session) = sso_host_and_responder_sessions();
        let stale_statement = build_outgoing_request_statement(
            &host_session,
            "statement-1".to_string(),
            vec![RemoteMessage {
                message_id: "remote-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
            }],
            elapsed_expiry(),
        )
        .unwrap();

        assert_eq!(
            decode_incoming_sso_request(&responder_session, &stale_statement).unwrap(),
            None
        );
    }

    #[test]
    fn responder_recovers_request_id_from_undecodable_messages() {
        let (host_session, responder_session) = sso_host_and_responder_sessions();
        let encrypted = encrypt_session_statement_data(
            &host_session,
            &SsoStatementData::Request {
                request_id: "statement-1".to_string(),
                data: vec![vec![0xFF, 0xFF, 0xFF]],
            },
        )
        .unwrap();
        let statement =
            build_signed_session_request_statement(&host_session, encrypted, fresh_expiry())
                .unwrap();

        let error = decode_incoming_sso_request(&responder_session, &statement).unwrap_err();
        assert_eq!(error.request_id.as_deref(), Some("statement-1"));
        assert!(error.reason.contains("invalid SSO remote message"));
    }

    fn response_ack_statement(session: &SsoSessionInfo, expiry: u64) -> Vec<u8> {
        let encrypted = encrypt_session_statement_data_with_nonce(
            session,
            &SsoStatementData::Response {
                request_id: "statement-1".to_string(),
                response_code: 0,
            },
            [9; AEAD_NONCE_LEN],
        )
        .unwrap();
        build_signed_statement(
            session,
            session.response_channel,
            session.session_id_own,
            encrypted,
            expiry,
        )
        .unwrap()
    }

    #[test]
    fn accepts_own_echoed_session_response_ack() {
        let session = session();
        let statement = response_ack_statement(&session, fresh_expiry());

        let decoded = decode_sso_session_statement(&session, &statement, "statement-1").unwrap();

        assert_eq!(decoded, Some(SsoSessionStatement::RequestAccepted));
    }

    /// A statement whose expiry is in the past must be ignored even when it
    /// would otherwise match the pending request (replay protection).
    #[test]
    fn ignores_expired_session_response_ack() {
        let session = session();
        let statement = response_ack_statement(&session, elapsed_expiry());

        let decoded = decode_sso_session_statement(&session, &statement, "statement-1").unwrap();

        assert_eq!(decoded, None);
    }
}
