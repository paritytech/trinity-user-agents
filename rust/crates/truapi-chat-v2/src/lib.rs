#![cfg_attr(not(feature = "std"), no_std)]

//! Chat v2 protocol primitives shared by native, mobile, and web hosts.
//!
//! Chat v2 is the deployed Statement Store chat protocol used by the current
//! iOS v2 and Android v2 applications. This crate intentionally stays pure:
//! no Statement Store I/O, no app persistence, and no UI state machine. It
//! centralizes the deterministic pieces that must not drift between hosts:
//! topic derivation, SCALE wire encoding, invite wrappers, and encrypted
//! request/response transport payloads.
extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use blake2::digest::consts::U32;
use blake2::digest::{Digest, KeyInit as BlakeKeyInit, Mac};
use blake2::{Blake2b, Blake2bMac};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

#[cfg(feature = "std")]
pub mod call_payload;
#[cfg(feature = "std")]
pub use call_payload::*;

/// Statement Store topic digest.
pub type Topic = [u8; 32];
fn encode_compact_u32(value: u32) -> Vec<u8> {
    if value < 1 << 6 {
        vec![(value as u8) << 2]
    } else if value < 1 << 14 {
        (((value as u16) << 2) | 0b01).to_le_bytes().to_vec()
    } else if value < 1 << 30 {
        ((value << 2) | 0b10).to_le_bytes().to_vec()
    } else {
        let mut out = Vec::with_capacity(5);
        out.push(0b11);
        out.extend_from_slice(&value.to_le_bytes());
        out
    }
}

fn decode_compact_u32(data: &[u8]) -> Result<(u32, usize), String> {
    let first = *data.first().ok_or_else(|| "compact: empty".to_string())?;
    match first & 0b11 {
        0 => Ok((u32::from(first >> 2), 1)),
        1 => {
            let bytes: [u8; 2] = data
                .get(..2)
                .ok_or_else(|| "compact: truncated 2-byte".to_string())?
                .try_into()
                .map_err(|_| "compact: truncated 2-byte".to_string())?;
            let value = u32::from(u16::from_le_bytes(bytes) >> 2);
            if value < 1 << 6 {
                return Err("compact: non-canonical 2-byte".into());
            }
            Ok((value, 2))
        }
        2 => {
            let bytes: [u8; 4] = data
                .get(..4)
                .ok_or_else(|| "compact: truncated 4-byte".to_string())?
                .try_into()
                .map_err(|_| "compact: truncated 4-byte".to_string())?;
            let value = u32::from_le_bytes(bytes) >> 2;
            if value < 1 << 14 {
                return Err("compact: non-canonical 4-byte".into());
            }
            Ok((value, 4))
        }
        3 => {
            if first >> 2 != 0 {
                return Err("compact: value exceeds u32".into());
            }
            let bytes: [u8; 4] = data
                .get(1..5)
                .ok_or_else(|| "compact: truncated big".to_string())?
                .try_into()
                .map_err(|_| "compact: truncated big".to_string())?;
            let value = u32::from_le_bytes(bytes);
            if value < 1 << 30 {
                return Err("compact: non-canonical big".into());
            }
            Ok((value, 5))
        }
        _ => unreachable!(),
    }
}

fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    Digest::update(&mut hasher, data);
    hasher.finalize().into()
}

fn blake2b_256_keyed(key: &[u8], data: &[u8]) -> Result<[u8; 32], ChatError> {
    if key.is_empty() {
        return Err(ChatError::KeyDerivationFailed(
            "BLAKE2b key must not be empty".into(),
        ));
    }
    let mut mac = <Blake2bMac<U32> as BlakeKeyInit>::new_from_slice(key).map_err(|_| {
        ChatError::KeyDerivationFailed(format!(
            "BLAKE2b key length must be 1..=64, got {}",
            key.len()
        ))
    })?;
    Mac::update(&mut mac, data);
    Ok(mac.finalize().into_bytes().into())
}

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("invalid chat v2 encoding: {0}")]
    InvalidEncoding(String),

    #[error("chat v2 key derivation failed: {0}")]
    KeyDerivationFailed(String),
}

/// Statement-store context used by the v2 first-contact chat-request protocol.
pub const CHAT_REQUEST_CONTEXT: &[u8] = b"chat-request";

/// Protocol epoch used by the v2 apps for day-partitioned request topics.
pub const PROTOCOL_EPOCH_SECONDS: u64 = 1_763_164_800;

/// Number of seconds per v2 chat-request pagination day.
pub const SECONDS_IN_DAY: u64 = 86_400;
/// Context bound into multi-device Chat v2 identity proofs.
pub const MULTI_DEVICE_CHAT_REQUEST_CONTEXT: &str = "mds-chat-request";
/// Domain separator for the opt-in context-bound cipher suite.
pub const CONTEXT_BOUND_CIPHER_DOMAIN: &[u8] = b"dotli-chat/context-bound/v1";
const CONTEXT_BOUND_INVITE_MAGIC: &[u8; 8] = b"DCHAT\x03\0\0";

/// Derive an X25519 public key without platform services.
pub fn x25519_public_key(private_key: &[u8; 32]) -> [u8; 32] {
    X25519PublicKey::from(&StaticSecret::from(*private_key)).to_bytes()
}

/// Perform X25519 agreement, rejecting non-contributory peer keys.
pub fn x25519_shared_secret(
    private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
) -> Result<[u8; 32], ChatError> {
    let shared = StaticSecret::from(*private_key)
        .diffie_hellman(&X25519PublicKey::from(*peer_public_key))
        .to_bytes();
    if shared == [0; 32] {
        return Err(ChatError::KeyDerivationFailed(
            "X25519 peer public key is non-contributory".into(),
        ));
    }
    Ok(shared)
}

/// Expand key material exactly as CryptoKit's
/// `hkdfDerivedSymmetricKey(SHA256, salt: empty, sharedInfo: empty, 32)`.
pub fn hkdf_sha256_32(input_key_material: &[u8]) -> Result<[u8; 32], ChatError> {
    let mut output = [0; 32];
    Hkdf::<Sha256>::new(Some(&[]), input_key_material)
        .expand(&[], &mut output)
        .map_err(|_| ChatError::KeyDerivationFailed("HKDF-SHA256 expansion failed".into()))?;
    Ok(output)
}

/// Derive the Chat v2 AEAD key shared with a peer X25519 public key.
pub fn x25519_hkdf_sha256_key(
    private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
) -> Result<[u8; 32], ChatError> {
    hkdf_sha256_32(&x25519_shared_secret(private_key, peer_public_key)?)
}
/// Encrypt with the opt-in context-bound suite.
///
/// The authenticated context binds the product/network identifier, sender,
/// recipient, route, and direction. The output remains nonce-prefixed.
pub fn context_bound_encrypt_with_nonce(
    input_key_material: &[u8],
    product_id: &str,
    sender_account_id: &[u8; 32],
    recipient_account_id: &[u8; 32],
    channel_id: &[u8; 32],
    plaintext: &[u8],
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let (key, aad) = context_bound_aead_material(
        input_key_material,
        product_id,
        sender_account_id,
        recipient_account_id,
        channel_id,
    )?;
    chacha20poly1305_encrypt_with_nonce_and_aad(&key, plaintext, nonce, &aad)
}

/// Decrypt with the exact context selected by the sender.
pub fn context_bound_decrypt(
    input_key_material: &[u8],
    product_id: &str,
    sender_account_id: &[u8; 32],
    recipient_account_id: &[u8; 32],
    channel_id: &[u8; 32],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ChatError> {
    let (key, aad) = context_bound_aead_material(
        input_key_material,
        product_id,
        sender_account_id,
        recipient_account_id,
        channel_id,
    )?;
    chacha20poly1305_decrypt_with_aad(&key, ciphertext, &aad)
}

/// X25519 agreement followed by context-bound encryption.
#[allow(clippy::too_many_arguments)]
pub fn x25519_context_bound_encrypt_with_nonce(
    private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    product_id: &str,
    sender_account_id: &[u8; 32],
    recipient_account_id: &[u8; 32],
    channel_id: &[u8; 32],
    plaintext: &[u8],
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    context_bound_encrypt_with_nonce(
        &x25519_shared_secret(private_key, peer_public_key)?,
        product_id,
        sender_account_id,
        recipient_account_id,
        channel_id,
        plaintext,
        nonce,
    )
}

/// X25519 agreement followed by context-bound decryption.
pub fn x25519_context_bound_decrypt(
    private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    product_id: &str,
    sender_account_id: &[u8; 32],
    recipient_account_id: &[u8; 32],
    channel_id: &[u8; 32],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ChatError> {
    context_bound_decrypt(
        &x25519_shared_secret(private_key, peer_public_key)?,
        product_id,
        sender_account_id,
        recipient_account_id,
        channel_id,
        ciphertext,
    )
}

fn context_bound_aead_material(
    input_key_material: &[u8],
    product_id: &str,
    sender_account_id: &[u8; 32],
    recipient_account_id: &[u8; 32],
    channel_id: &[u8; 32],
) -> Result<([u8; 32], Vec<u8>), ChatError> {
    let product_id_len = u32::try_from(product_id.len())
        .map_err(|_| ChatError::InvalidEncoding("product identifier is too long".into()))?;
    let mut aad =
        Vec::with_capacity(CONTEXT_BOUND_CIPHER_DOMAIN.len() + 4 + product_id.len() + 32 + 32 + 32);
    aad.extend_from_slice(CONTEXT_BOUND_CIPHER_DOMAIN);
    aad.extend_from_slice(&product_id_len.to_le_bytes());
    aad.extend_from_slice(product_id.as_bytes());
    aad.extend_from_slice(sender_account_id);
    aad.extend_from_slice(recipient_account_id);
    aad.extend_from_slice(channel_id);
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(CONTEXT_BOUND_CIPHER_DOMAIN), input_key_material)
        .expand(&aad, &mut key)
        .map_err(|_| ChatError::KeyDerivationFailed("context-bound HKDF-SHA256 failed".into()))?;
    Ok((key, aad))
}

/// Build the keyed proof binding an identity account to a product device.
///
/// The keyed BLAKE2b-256 input is the raw identity account, raw device account,
/// then the SCALE string `mds-chat-request`.
pub fn v2_identity_proof(
    identity_account_id: &[u8; 32],
    device_account_id: &[u8; 32],
    shared_secret: &[u8; 32],
) -> Result<[u8; 32], ChatError> {
    let mut payload = Vec::with_capacity(64 + 1 + MULTI_DEVICE_CHAT_REQUEST_CONTEXT.len());
    payload.extend_from_slice(identity_account_id);
    payload.extend_from_slice(device_account_id);
    encode_string(&mut payload, MULTI_DEVICE_CHAT_REQUEST_CONTEXT)?;
    blake2b_256_keyed(shared_secret, &payload)
}

/// Supported subset of the v2 remote chat message content enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2ChatMessageContent {
    /// Plain text content. V2 enum index 0.
    Text(String),
    /// Token content. V2 enum index 1.
    Token {
        token: Vec<u8>,
        platform: V2PushPlatform,
    },
    /// Legacy send payload for iOS v2 compatibility. V2 enum index 2.
    SendLegacy {
        amount: String,
        block_hash: Vec<u8>,
        extrinsic_hash: Vec<u8>,
    },
    /// Contact added notification (deprecated). V2 enum index 3.
    ContactAdded,
    /// Emoji reaction to a message. V2 enum index 4.
    Reacted { message_id: String, emoji: String },
    /// Emoji reaction removal. V2 enum index 5.
    ReactionRemoved { message_id: String, emoji: String },
    /// Reply to a message with own content. V2 enum index 7.
    Reply {
        message_id: String,
        text: Option<String>,
        attachments: Option<Vec<V2FileVariant>>,
    },
    /// WebRTC/data-channel offer signaling. V2 enum index 8.
    DataChannelOffer {
        sdp: Vec<u8>,
        purpose: V2DataChannelPurpose,
    },
    /// WebRTC/data-channel answer signaling. V2 enum index 9.
    DataChannelAnswer { offer_id: String, sdp: Vec<u8> },
    /// WebRTC/data-channel ICE candidates signaling. V2 enum index 10.
    DataChannelCandidates { offer_id: String, sdp: Vec<u8> },
    /// WebRTC/data-channel closed signaling. V2 enum index 11.
    DataChannelClosed { offer_id: String },
    /// Edited message content. V2 enum index 12.
    Edited {
        message_id: String,
        new_text: Option<String>,
        attachments: Option<Vec<V2FileVariant>>,
    },
    /// User left the chat. V2 enum index 13.
    LeftChat,
    /// Chat request acceptance content. V2 enum index 14.
    ChatAccepted { request_id: String },
    /// Rich text message. V2 enum index 15.
    RichText {
        text: Option<String>,
        attachments: Option<Vec<V2FileVariant>>,
    },
    /// Coinage payment. V2 enum index 16.
    CoinageSend {
        total_value: String,
        coin_keys: Vec<Vec<u8>>,
    },
    /// A device was added to the identity. V2 enum index 17.
    DeviceAdded {
        statement_account_id: Vec<u8>,
        encryption_public_key: Vec<u8>,
    },
    /// A device was removed from the identity. V2 enum index 18.
    DeviceRemoved { statement_account_id: Vec<u8> },
    /// Reference to a compacted message batch. V2 enum index 19.
    CompactedMessages {
        claim_identifier: Vec<u8>,
        claim_ticket: Vec<u8>,
        node: V2NodeEndpoint,
    },
    /// Multi-device chat acceptance. V2 wire enum index 20.
    MultiChatAccepted {
        request_id: String,
        device: V2PeerDevice,
    },
    /// A profile reference the sender's host discloses to this contact, or
    /// `None` to withdraw it. Host-originated and host-consumed: products
    /// never send or see it. V2 wire enum index 21.
    ProfileReference {
        discloser_product_id: String,
        reference: Option<String>,
    },
    /// The envelope was valid enough to recover id/timestamp, but the versioned
    /// content wrapper is not yet represented by this SDK surface.
    UnsupportedVersion { version_index: u8 },
    /// The V1 envelope was valid enough to recover id/timestamp, but its
    /// content enum is not yet represented by this SDK surface.
    UnsupportedContent { content_index: u8 },
}

/// V2 chat message statement payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatMessage {
    pub message_id: String,
    pub timestamp: u64,
    pub content: V2ChatMessageContent,
}
/// Fixed-width device record carried by multi-device Chat v2 messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2PeerDevice {
    pub statement_account_id: [u8; 32],
    pub encryption_public_key: [u8; 32],
}

/// Endpoint for a Chat v2 attachment or compacted message batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2NodeEndpoint {
    /// Secure WebSocket URL. Endpoint enum index 0.
    WssUrl(String),
}

/// Attachment transport. Native SCALE enum index 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2FileVariant {
    P2pMixnet(V2P2pMixnetFile),
}

/// Private HOP reference; identifiers and claim tickets are exactly 32 bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct V2P2pMixnetFile {
    pub identifier: Vec<u8>,
    pub claim_ticket: Vec<u8>,
    pub node: V2NodeEndpoint,
    pub meta: V2FileMeta,
}

impl core::fmt::Debug for V2P2pMixnetFile {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("V2P2pMixnetFile")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

impl Drop for V2P2pMixnetFile {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.claim_ticket);
    }
}

/// Native SCALE metadata indices: General = 0, Image = 1, Video = 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2FileMeta {
    General(V2GeneralFileMeta),
    Image(V2ImageFileMeta),
    Video(V2VideoFileMeta),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2GeneralFileMeta {
    pub mime_type: String,
    pub file_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ImageFileMeta {
    pub general: V2GeneralFileMeta,
    pub width: u32,
    pub height: u32,
    /// Native UTF-8 BlurHash bytes, not an encoded image.
    pub thumbnail: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2VideoFileMeta {
    pub general: V2GeneralFileMeta,
    pub duration: u32,
    /// Native UTF-8 BlurHash bytes, not an encoded image.
    pub thumbnail: Option<Vec<u8>>,
}

/// Shared data-channel purpose enum used by the v2 apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2DataChannelPurpose {
    Audio,
    Video,
}

/// Transport-neutral call signaling model shared across v2 chat and host extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2CallSignal {
    Offer {
        offer_id: String,
        sdp: Vec<u8>,
        purpose: V2DataChannelPurpose,
    },
    Answer {
        offer_id: String,
        sdp: Vec<u8>,
    },
    Candidates {
        offer_id: String,
        sdp: Vec<u8>,
    },
    Closed {
        offer_id: String,
    },
}

impl V2ChatMessage {
    /// Lift supported v2 data-channel chat content into the shared call-signal model.
    pub fn as_call_signal(&self) -> Option<V2CallSignal> {
        match &self.content {
            V2ChatMessageContent::DataChannelOffer { sdp, purpose } => Some(V2CallSignal::Offer {
                offer_id: self.message_id.clone(),
                sdp: sdp.clone(),
                purpose: *purpose,
            }),
            V2ChatMessageContent::DataChannelAnswer { offer_id, sdp } => {
                Some(V2CallSignal::Answer {
                    offer_id: offer_id.clone(),
                    sdp: sdp.clone(),
                })
            }
            V2ChatMessageContent::DataChannelCandidates { offer_id, sdp } => {
                Some(V2CallSignal::Candidates {
                    offer_id: offer_id.clone(),
                    sdp: sdp.clone(),
                })
            }
            V2ChatMessageContent::DataChannelClosed { offer_id } => Some(V2CallSignal::Closed {
                offer_id: offer_id.clone(),
            }),
            _ => None,
        }
    }
}

/// Push platform enum used inside first-contact chat requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2PushPlatform {
    Android,
    Ios,
    IosVoip,
}

/// Optional push token bundled with a first-contact chat request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2PushToken {
    pub token: Vec<u8>,
    pub platform: V2PushPlatform,
}

/// Supported subset of the v2 first-contact request content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequestMessage {
    pub message_id: String,
    pub timestamp: u64,
    pub push_token: Option<V2PushToken>,
    pub welcome_text: Option<String>,
}
/// Raw fixed-width keyed proof carried by request content V2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequestIdentityProof {
    pub identity_account_id: [u8; 32],
    pub proof: [u8; 32],
}

/// Current iOS multi-device first-contact request content (version index 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequestContentV2 {
    pub identity_proof: V2ChatRequestIdentityProof,
    pub device_enc_pub_key: [u8; 32],
    pub push_token: Option<V2PushToken>,
    pub welcome_text: Option<String>,
}

/// First-contact request message carrying request content V2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequestMessageV2 {
    pub message_id: String,
    pub timestamp: u64,
    pub content: V2ChatRequestContentV2,
}

/// Inner sr25519 proof carried by a first-contact chat request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequestProof {
    pub signature: Vec<u8>,
    pub signer: Vec<u8>,
}

/// Decrypted first-contact chat request payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequest {
    pub message: V2ChatRequestMessage,
    pub proof: V2ChatRequestProof,
}

/// Decrypted first-contact payload carrying current request content V2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2ChatRequestV2 {
    pub message: V2ChatRequestMessageV2,
    pub proof: V2ChatRequestProof,
}

/// Encrypted transport wrapper for first-contact chat requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2EncryptedChatRequest {
    pub encryption_pubkey: Vec<u8>,
    pub encrypted_request: Vec<u8>,
}

/// Per-recipient wrapped one-shot key in a multi-device transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2RequestDeviceInfo {
    pub statement_account_id: [u8; 32],
    pub encrypted_key: Vec<u8>,
}

/// Multi-device request envelope, StatementData index 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2MultiDeviceRequest {
    pub encrypted_request: Vec<u8>,
    pub devices_info: Vec<V2RequestDeviceInfo>,
}

/// Multi-device response envelope, StatementData index 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2MultiDeviceResponse {
    pub encrypted_response: Vec<u8>,
    pub devices_info: Vec<V2RequestDeviceInfo>,
}

/// Bare MessageExchange request encrypted inside a multi-device request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2MessageExchangeRequest {
    pub request_id: String,
    pub messages: Vec<Vec<u8>>,
}

/// Bare MessageExchange response encrypted inside a multi-device response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2MessageExchangeResponse {
    pub request_id: String,
    pub response_code: u8,
}

/// Ongoing v2 statement-store transport payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2StatementTransportData {
    Request {
        request_id: String,
        messages: Vec<Vec<u8>>,
    },
    Response {
        request_id: String,
        response_code: u8,
    },
    MultiRequest(V2MultiDeviceRequest),
    MultiResponse(V2MultiDeviceResponse),
}

/// Calculate the v2 day number from a Unix timestamp in seconds.
///
/// Returns `None` for timestamps before the v2 protocol epoch.
pub fn chat_request_day_from_unix(unix_timestamp_seconds: u64) -> Option<u64> {
    unix_timestamp_seconds
        .checked_sub(PROTOCOL_EPOCH_SECONDS)
        .map(|relative| relative / SECONDS_IN_DAY)
}

/// Derive the full-history first-contact request topic for an acceptor account.
///
/// Matches v2 iOS/Android:
/// `blake2b256(scale(Data("chat-request")) || scale(Data(acceptor_account_id)))`.
pub fn chat_request_full_topic(acceptor_account_id: &[u8; 32]) -> Topic {
    derive_chat_request_topic(CHAT_REQUEST_CONTEXT, acceptor_account_id, &[])
}

/// Derive the day-partitioned first-contact request topic for an acceptor account.
///
/// Matches v2 iOS/Android:
/// `blake2b256(scale(Data("chat-request")) || scale(Data(acceptor_account_id)) || scale(UInt64(day)))`.
pub fn chat_request_day_topic(acceptor_account_id: &[u8; 32], day: u64) -> Topic {
    chat_request_day_topic_with_context(CHAT_REQUEST_CONTEXT, acceptor_account_id, day)
}

fn chat_request_day_topic_with_context(
    context: &[u8],
    acceptor_account_id: &[u8; 32],
    day: u64,
) -> Topic {
    let day_bytes = day.to_le_bytes();
    derive_chat_request_topic(context, acceptor_account_id, &day_bytes)
}

fn derive_chat_request_topic(
    context: &[u8],
    acceptor_account_id: &[u8; 32],
    suffix: &[u8],
) -> Topic {
    let mut input = Vec::with_capacity(
        compact_len_size(context.len() as u32)
            + context.len()
            + compact_len_size(acceptor_account_id.len() as u32)
            + acceptor_account_id.len()
            + suffix.len(),
    );
    input.extend_from_slice(&encode_compact_u32(context.len() as u32));
    input.extend_from_slice(context);
    input.extend_from_slice(&encode_compact_u32(acceptor_account_id.len() as u32));
    input.extend_from_slice(acceptor_account_id);
    input.extend_from_slice(suffix);
    blake2b_256(&input)
}

fn compact_len_size(value: u32) -> usize {
    encode_compact_u32(value).len()
}

/// Build v2 session id parameters:
/// `requester_account_id || acceptor_account_id || "/" || requester_pin || "/" || acceptor_pin`.
pub fn chat_request_session_id_params(
    requester_account_id: &[u8; 32],
    requester_pin: Option<&str>,
    acceptor_account_id: &[u8; 32],
    acceptor_pin: Option<&str>,
) -> Vec<u8> {
    let requester_pin = requester_pin.unwrap_or("").as_bytes();
    let acceptor_pin = acceptor_pin.unwrap_or("").as_bytes();
    let mut out = Vec::with_capacity(32 + 32 + 1 + requester_pin.len() + 1 + acceptor_pin.len());
    out.extend_from_slice(requester_account_id);
    out.extend_from_slice(acceptor_account_id);
    out.push(b'/');
    out.extend_from_slice(requester_pin);
    out.push(b'/');
    out.extend_from_slice(acceptor_pin);
    out
}

/// Derive the session fallback first-contact request topic.
///
/// Matches v2 Android:
/// `keyed_blake2b256(shared_secret, "chat-request" || session_id_params)`.
pub fn chat_request_session_topic(
    shared_secret: &[u8],
    requester_account_id: &[u8; 32],
    requester_pin: Option<&str>,
    acceptor_account_id: &[u8; 32],
    acceptor_pin: Option<&str>,
) -> Result<Topic, ChatError> {
    if shared_secret.len() != 32 {
        return Err(ChatError::KeyDerivationFailed(format!(
            "shared_secret must be 32 bytes, got {}",
            shared_secret.len()
        )));
    }

    let session_id_params = chat_request_session_id_params(
        requester_account_id,
        requester_pin,
        acceptor_account_id,
        acceptor_pin,
    );
    let mut input = Vec::with_capacity(CHAT_REQUEST_CONTEXT.len() + session_id_params.len());
    input.extend_from_slice(CHAT_REQUEST_CONTEXT);
    input.extend_from_slice(&session_id_params);
    blake2b_256_keyed(shared_secret, &input)
}

/// Derive an ongoing identity session id in one account direction.
pub fn chat_identity_session_id(
    shared_secret: &[u8; 32],
    first_account_id: &[u8; 32],
    first_pin: Option<&str>,
    second_account_id: &[u8; 32],
    second_pin: Option<&str>,
) -> Result<[u8; 32], ChatError> {
    let params =
        chat_request_session_id_params(first_account_id, first_pin, second_account_id, second_pin);
    let mut input = Vec::with_capacity(7 + params.len());
    input.extend_from_slice(b"session");
    input.extend_from_slice(&params);
    blake2b_256_keyed(shared_secret, &input)
}

/// Derive the request topic for an ongoing identity session.
pub fn chat_identity_request_topic(session_id: &[u8; 32]) -> Result<Topic, ChatError> {
    blake2b_256_keyed(session_id, b"request")
}

/// Derive the response topic for an ongoing identity session.
pub fn chat_identity_response_topic(session_id: &[u8; 32]) -> Result<Topic, ChatError> {
    blake2b_256_keyed(session_id, b"response")
}

/// Encode a v2 text message statement payload.
pub fn encode_text_message(
    message_id: &str,
    timestamp: u64,
    text: &str,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(0);
        encode_string(out, text)
    })
}

/// Encode a v2 chat-accepted message statement payload.
pub fn encode_chat_accepted_message(
    message_id: &str,
    timestamp: u64,
    request_id: &str,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(14);
        encode_string(out, request_id)
    })
}

/// Encode a v2 token message statement payload.
pub fn encode_token_message(
    message_id: &str,
    timestamp: u64,
    token: &[u8],
    platform: V2PushPlatform,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(1);
        encode_bytes(out, token)?;
        out.push(push_platform_index(platform));
        Ok(())
    })
}

/// Encode a v2 legacy send message statement payload.
pub fn encode_send_legacy_message(
    message_id: &str,
    timestamp: u64,
    amount: &str,
    block_hash: &[u8],
    extrinsic_hash: &[u8],
) -> Result<Vec<u8>, ChatError> {
    if block_hash.len() != 32 {
        return Err(ChatError::InvalidEncoding(format!(
            "legacy send block_hash must be 32 bytes, got {}",
            block_hash.len()
        )));
    }
    if extrinsic_hash.len() != 32 {
        return Err(ChatError::InvalidEncoding(format!(
            "legacy send extrinsic_hash must be 32 bytes, got {}",
            extrinsic_hash.len()
        )));
    }
    encode_message(message_id, timestamp, |out| {
        out.push(2);
        encode_balance(out, amount)?;
        out.extend_from_slice(block_hash);
        out.extend_from_slice(extrinsic_hash);
        Ok(())
    })
}

/// Encode a v2 contact-added message statement payload.
pub fn encode_contact_added_message(
    message_id: &str,
    timestamp: u64,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(3);
        Ok(())
    })
}

/// Encode a v2 reacted message statement payload.
pub fn encode_reacted_message(
    message_id: &str,
    timestamp: u64,
    referenced_message_id: &str,
    emoji: &str,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(4);
        encode_string(out, referenced_message_id)?;
        encode_string(out, emoji)
    })
}

/// Encode a v2 reaction-removed message statement payload.
pub fn encode_reaction_removed_message(
    message_id: &str,
    timestamp: u64,
    referenced_message_id: &str,
    emoji: &str,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(5);
        encode_string(out, referenced_message_id)?;
        encode_string(out, emoji)
    })
}

/// Encode a v2 reply message statement payload.
pub fn encode_reply_message(
    message_id: &str,
    timestamp: u64,
    referenced_message_id: &str,
    text: Option<&str>,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(7);
        encode_string(out, referenced_message_id)?;
        encode_rich_text(out, text, None)
    })
}

/// Encode a v2 edited message statement payload.
pub fn encode_edited_message(
    message_id: &str,
    timestamp: u64,
    referenced_message_id: &str,
    new_text: Option<&str>,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(12);
        encode_string(out, referenced_message_id)?;
        encode_rich_text(out, new_text, None)
    })
}

/// Encode a v2 left-chat message statement payload.
pub fn encode_left_chat_message(message_id: &str, timestamp: u64) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(13);
        Ok(())
    })
}

/// Encode a v2 rich-text message statement payload.
pub fn encode_rich_text_message(
    message_id: &str,
    timestamp: u64,
    text: Option<&str>,
    attachments: Option<&[V2FileVariant]>,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(15);
        encode_rich_text(out, text, attachments)
    })
}

/// Encode a v2 coinage-send message statement payload.
pub fn encode_coinage_send_message(
    message_id: &str,
    timestamp: u64,
    total_value: &str,
    coin_keys: &[Vec<u8>],
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(16);
        encode_balance(out, total_value)?;
        encode_vec_of_bytes(out, coin_keys)
    })
}
/// Encode a v2 device-added message (content index 17).
pub fn encode_device_added_message(
    message_id: &str,
    timestamp: u64,
    statement_account_id: &[u8],
    encryption_public_key: &[u8],
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(17);
        encode_bytes(out, statement_account_id)?;
        encode_bytes(out, encryption_public_key)
    })
}

/// Encode a v2 device-removed message (content index 18).
pub fn encode_device_removed_message(
    message_id: &str,
    timestamp: u64,
    statement_account_id: &[u8],
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(18);
        encode_bytes(out, statement_account_id)
    })
}

/// Encode a v2 profile-reference message (content index 21).
pub fn encode_profile_reference_message(
    message_id: &str,
    timestamp: u64,
    discloser_product_id: &str,
    reference: Option<&str>,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(21);
        encode_string(out, discloser_product_id)?;
        match reference {
            Some(reference) => {
                out.push(1);
                encode_string(out, reference)
            }
            None => {
                out.push(0);
                Ok(())
            }
        }
    })
}

/// Encode a v2 compacted-messages reference (content index 19).
pub fn encode_compacted_messages_message(
    message_id: &str,
    timestamp: u64,
    claim_identifier: &[u8],
    claim_ticket: &[u8],
    node: &V2NodeEndpoint,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(19);
        encode_bytes(out, claim_identifier)?;
        encode_bytes(out, claim_ticket)?;
        match node {
            V2NodeEndpoint::WssUrl(url) => {
                out.push(0);
                encode_string(out, url)
            }
        }
    })
}

/// Encode a v2 multi-device chat acceptance (wire content index 20).
pub fn encode_multi_chat_accepted_message(
    message_id: &str,
    timestamp: u64,
    request_id: &str,
    device: &V2PeerDevice,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(20);
        encode_string(out, request_id)?;
        out.extend_from_slice(&device.statement_account_id);
        out.extend_from_slice(&device.encryption_public_key);
        Ok(())
    })
}

/// Encode a transport-neutral call signal into the v2 chat message wire format.
pub fn encode_call_signal_message(
    message_id: &str,
    timestamp: u64,
    signal: &V2CallSignal,
) -> Result<Vec<u8>, ChatError> {
    match signal {
        V2CallSignal::Offer { sdp, purpose, .. } => {
            encode_data_channel_offer_message(message_id, timestamp, sdp, *purpose)
        }
        V2CallSignal::Answer { offer_id, sdp } => {
            encode_data_channel_answer_message(message_id, timestamp, offer_id, sdp)
        }
        V2CallSignal::Candidates { offer_id, sdp } => {
            encode_data_channel_candidates_message(message_id, timestamp, offer_id, sdp)
        }
        V2CallSignal::Closed { offer_id } => {
            encode_data_channel_closed_message(message_id, timestamp, offer_id)
        }
    }
}

/// Encode a v2 data-channel-offer message statement payload.
pub fn encode_data_channel_offer_message(
    message_id: &str,
    timestamp: u64,
    sdp: &[u8],
    purpose: V2DataChannelPurpose,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(8);
        encode_bytes(out, sdp)?;
        out.push(match purpose {
            V2DataChannelPurpose::Audio => 0,
            V2DataChannelPurpose::Video => 1,
        });
        Ok(())
    })
}

/// Encode a v2 data-channel-answer message statement payload.
pub fn encode_data_channel_answer_message(
    message_id: &str,
    timestamp: u64,
    offer_id: &str,
    sdp: &[u8],
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(9);
        encode_string(out, offer_id)?;
        encode_bytes(out, sdp)
    })
}

/// Encode a v2 data-channel-candidates message statement payload.
pub fn encode_data_channel_candidates_message(
    message_id: &str,
    timestamp: u64,
    offer_id: &str,
    sdp: &[u8],
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(10);
        encode_string(out, offer_id)?;
        encode_bytes(out, sdp)
    })
}

/// Encode a v2 data-channel-closed message statement payload.
pub fn encode_data_channel_closed_message(
    message_id: &str,
    timestamp: u64,
    offer_id: &str,
) -> Result<Vec<u8>, ChatError> {
    encode_message(message_id, timestamp, |out| {
        out.push(11);
        encode_string(out, offer_id)
    })
}

/// Decode a v2 chat message statement payload.
///
/// The decoder preserves `message_id` and `timestamp` for unsupported content
/// variants so host apps can store/display an unsupported placeholder.
pub fn decode_message(data: &[u8]) -> Result<V2ChatMessage, ChatError> {
    let mut cursor = Cursor::new(data);
    let message_id = cursor.read_string("message_id")?;
    let timestamp = cursor.read_u64("timestamp")?;
    let version_index = cursor.read_u8("version_index")?;
    if version_index != 0 {
        return Ok(V2ChatMessage {
            message_id,
            timestamp,
            content: V2ChatMessageContent::UnsupportedVersion { version_index },
        });
    }

    let content_index = cursor.read_u8("content_index")?;
    let content = match content_index {
        0 => {
            let text = cursor.read_string("text")?;
            cursor.finish()?;
            V2ChatMessageContent::Text(text)
        }
        1 => {
            let token = cursor.read_bytes("push_token")?;
            let platform = decode_push_platform(cursor.read_u8("push_platform")?)?;
            cursor.finish()?;
            V2ChatMessageContent::Token { token, platform }
        }
        2 => {
            let amount = cursor.read_balance("amount")?;
            let block_hash = cursor.read_exact(32, "block_hash")?.to_vec();
            let extrinsic_hash = cursor.read_exact(32, "extrinsic_hash")?.to_vec();
            cursor.finish()?;
            V2ChatMessageContent::SendLegacy {
                amount,
                block_hash,
                extrinsic_hash,
            }
        }
        3 => {
            cursor.finish()?;
            V2ChatMessageContent::ContactAdded
        }
        4 => {
            let message_id = cursor.read_string("referenced_message_id")?;
            let emoji = cursor.read_string("emoji")?;
            cursor.finish()?;
            V2ChatMessageContent::Reacted { message_id, emoji }
        }
        5 => {
            let message_id = cursor.read_string("referenced_message_id")?;
            let emoji = cursor.read_string("emoji")?;
            cursor.finish()?;
            V2ChatMessageContent::ReactionRemoved { message_id, emoji }
        }
        7 => {
            let message_id = cursor.read_string("referenced_message_id")?;
            let (text, attachments) = decode_rich_text(&mut cursor)?;
            cursor.finish()?;
            V2ChatMessageContent::Reply {
                message_id,
                text,
                attachments,
            }
        }
        8 => {
            let sdp = cursor.read_bytes("sdp")?;
            let purpose = match cursor.read_u8("purpose")? {
                0 => V2DataChannelPurpose::Audio,
                1 => V2DataChannelPurpose::Video,
                value => {
                    return Err(ChatError::InvalidEncoding(format!(
                        "unsupported data channel purpose {value}"
                    )));
                }
            };
            cursor.finish()?;
            V2ChatMessageContent::DataChannelOffer { sdp, purpose }
        }
        9 => {
            let offer_id = cursor.read_string("offer_id")?;
            let sdp = cursor.read_bytes("sdp")?;
            cursor.finish()?;
            V2ChatMessageContent::DataChannelAnswer { offer_id, sdp }
        }
        10 => {
            let offer_id = cursor.read_string("offer_id")?;
            let sdp = cursor.read_bytes("sdp")?;
            cursor.finish()?;
            V2ChatMessageContent::DataChannelCandidates { offer_id, sdp }
        }
        11 => {
            let offer_id = cursor.read_string("offer_id")?;
            cursor.finish()?;
            V2ChatMessageContent::DataChannelClosed { offer_id }
        }
        12 => {
            let message_id = cursor.read_string("referenced_message_id")?;
            let (new_text, attachments) = decode_rich_text(&mut cursor)?;
            cursor.finish()?;
            V2ChatMessageContent::Edited {
                message_id,
                new_text,
                attachments,
            }
        }
        13 => {
            cursor.finish()?;
            V2ChatMessageContent::LeftChat
        }
        14 => {
            let request_id = cursor.read_string("request_id")?;
            cursor.finish()?;
            V2ChatMessageContent::ChatAccepted { request_id }
        }
        15 => {
            let (text, attachments) = decode_rich_text(&mut cursor)?;
            cursor.finish()?;
            V2ChatMessageContent::RichText { text, attachments }
        }
        16 => {
            let total_value = cursor.read_balance("total_value")?;
            let coin_keys = cursor.read_vec_of_bytes("coin_keys")?;
            cursor.finish()?;
            V2ChatMessageContent::CoinageSend {
                total_value,
                coin_keys,
            }
        }
        17 => {
            let statement_account_id = cursor.read_bytes("statement_account_id")?;
            let encryption_public_key = cursor.read_bytes("encryption_public_key")?;
            cursor.finish()?;
            V2ChatMessageContent::DeviceAdded {
                statement_account_id,
                encryption_public_key,
            }
        }
        18 => {
            let statement_account_id = cursor.read_bytes("statement_account_id")?;
            cursor.finish()?;
            V2ChatMessageContent::DeviceRemoved {
                statement_account_id,
            }
        }
        19 => {
            let claim_identifier = cursor.read_bytes("claim_identifier")?;
            let claim_ticket = cursor.read_bytes("claim_ticket")?;
            let node = match cursor.read_u8("node_endpoint")? {
                0 => V2NodeEndpoint::WssUrl(cursor.read_string("wss_url")?),
                value => {
                    return Err(ChatError::InvalidEncoding(format!(
                        "unsupported node endpoint {value}"
                    )));
                }
            };
            cursor.finish()?;
            V2ChatMessageContent::CompactedMessages {
                claim_identifier,
                claim_ticket,
                node,
            }
        }
        20 => {
            let request_id = cursor.read_string("request_id")?;
            let statement_account_id = cursor.read_array_32("device_statement_account_id")?;
            let encryption_public_key = cursor.read_array_32("device_encryption_public_key")?;
            cursor.finish()?;
            V2ChatMessageContent::MultiChatAccepted {
                request_id,
                device: V2PeerDevice {
                    statement_account_id,
                    encryption_public_key,
                },
            }
        }
        21 => {
            let discloser_product_id = cursor.read_string("discloser_product_id")?;
            let reference = match cursor.read_u8("reference_option")? {
                0 => None,
                1 => Some(cursor.read_string("reference")?),
                value => {
                    return Err(ChatError::InvalidEncoding(format!(
                        "invalid profile reference option {value}"
                    )));
                }
            };
            cursor.finish()?;
            V2ChatMessageContent::ProfileReference {
                discloser_product_id,
                reference,
            }
        }
        index => V2ChatMessageContent::UnsupportedContent {
            content_index: index,
        },
    };

    Ok(V2ChatMessage {
        message_id,
        timestamp,
        content,
    })
}

/// Encode a first-contact chat request message.
pub fn encode_chat_request_message(
    message_id: &str,
    timestamp: u64,
    push_token: Option<&V2PushToken>,
    welcome_text: Option<&str>,
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    encode_string(&mut out, message_id)?;
    out.extend_from_slice(&timestamp.to_le_bytes());
    out.push(0); // VersionedRequestContent::V1

    encode_optional_push_token(&mut out, push_token)?;
    encode_optional_rich_text(&mut out, welcome_text)?;

    Ok(out)
}

/// Decode a first-contact chat request message.
pub fn decode_chat_request_message(data: &[u8]) -> Result<V2ChatRequestMessage, ChatError> {
    let mut cursor = Cursor::new(data);
    let message_id = cursor.read_string("message_id")?;
    let timestamp = cursor.read_u64("timestamp")?;
    let version_index = cursor.read_u8("version_index")?;
    if version_index != 0 {
        return Err(ChatError::InvalidEncoding(format!(
            "unsupported v2 chat request version index {version_index}"
        )));
    }

    let push_token = decode_optional_push_token(&mut cursor)?;
    let welcome_text = decode_optional_rich_text(&mut cursor)?;
    cursor.finish()?;

    Ok(V2ChatRequestMessage {
        message_id,
        timestamp,
        push_token,
        welcome_text,
    })
}

/// Encode the proof payload that the requester signs for a first-contact chat request.
pub fn encode_chat_request_proof_payload(
    message: &V2ChatRequestMessage,
    acceptor_account_id: &[u8; 32],
) -> Result<Vec<u8>, ChatError> {
    let mut out = encode_chat_request_message(
        &message.message_id,
        message.timestamp,
        message.push_token.as_ref(),
        message.welcome_text.as_deref(),
    )?;
    encode_bytes(&mut out, acceptor_account_id)?;
    Ok(out)
}

/// Encode a decrypted first-contact chat request payload.
pub fn encode_chat_request(request: &V2ChatRequest) -> Result<Vec<u8>, ChatError> {
    if request.proof.signature.len() != 64 {
        return Err(ChatError::InvalidEncoding(format!(
            "chat request proof signature must be 64 bytes, got {}",
            request.proof.signature.len()
        )));
    }
    if request.proof.signer.len() != 32 {
        return Err(ChatError::InvalidEncoding(format!(
            "chat request proof signer must be 32 bytes, got {}",
            request.proof.signer.len()
        )));
    }

    let mut out = encode_chat_request_message(
        &request.message.message_id,
        request.message.timestamp,
        request.message.push_token.as_ref(),
        request.message.welcome_text.as_deref(),
    )?;
    out.push(0); // StatementProof::sr25519
    out.extend_from_slice(&request.proof.signature);
    out.extend_from_slice(&request.proof.signer);
    Ok(out)
}
/// Encode a first-contact request carrying current multi-device content V2.
pub fn encode_chat_request_message_v2(
    message: &V2ChatRequestMessageV2,
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    encode_string(&mut out, &message.message_id)?;
    out.extend_from_slice(&message.timestamp.to_le_bytes());
    out.push(1);
    out.extend_from_slice(&message.content.identity_proof.identity_account_id);
    out.extend_from_slice(&message.content.identity_proof.proof);
    out.extend_from_slice(&message.content.device_enc_pub_key);
    encode_optional_push_token(&mut out, message.content.push_token.as_ref())?;
    encode_optional_rich_text(&mut out, message.content.welcome_text.as_deref())?;
    Ok(out)
}

/// Decode a first-contact request carrying current multi-device content V2.
pub fn decode_chat_request_message_v2(data: &[u8]) -> Result<V2ChatRequestMessageV2, ChatError> {
    let mut cursor = Cursor::new(data);
    let message = decode_chat_request_message_v2_cursor(&mut cursor)?;
    cursor.finish()?;
    Ok(message)
}

/// Encode the sr25519 proof payload for request content V2.
pub fn encode_chat_request_v2_proof_payload(
    message: &V2ChatRequestMessageV2,
    acceptor_account_id: &[u8; 32],
) -> Result<Vec<u8>, ChatError> {
    let mut out = encode_chat_request_message_v2(message)?;
    encode_bytes(&mut out, acceptor_account_id)?;
    Ok(out)
}

/// Encode a decrypted first-contact request carrying request content V2.
pub fn encode_chat_request_v2(request: &V2ChatRequestV2) -> Result<Vec<u8>, ChatError> {
    validate_chat_request_proof(&request.proof)?;
    let mut out = encode_chat_request_message_v2(&request.message)?;
    out.push(0); // StatementProof::sr25519
    out.extend_from_slice(&request.proof.signature);
    out.extend_from_slice(&request.proof.signer);
    Ok(out)
}

/// Decode a decrypted first-contact request carrying request content V2.
pub fn decode_chat_request_v2(data: &[u8]) -> Result<V2ChatRequestV2, ChatError> {
    let mut cursor = Cursor::new(data);
    let message = decode_chat_request_message_v2_cursor(&mut cursor)?;
    let proof = decode_chat_request_proof(&mut cursor)?;
    cursor.finish()?;
    Ok(V2ChatRequestV2 { message, proof })
}

/// Decode a decrypted first-contact chat request payload.
pub fn decode_chat_request(data: &[u8]) -> Result<V2ChatRequest, ChatError> {
    let mut cursor = Cursor::new(data);
    let message = decode_chat_request_message_cursor(&mut cursor)?;
    let proof_index = cursor.read_u8("proof_index")?;
    if proof_index != 0 {
        return Err(ChatError::InvalidEncoding(format!(
            "unsupported chat request proof index {proof_index}"
        )));
    }
    let signature = cursor.read_exact(64, "proof_signature")?.to_vec();
    let signer = cursor.read_exact(32, "proof_signer")?.to_vec();
    cursor.finish()?;

    Ok(V2ChatRequest {
        message,
        proof: V2ChatRequestProof { signature, signer },
    })
}

/// Encode the encrypted outer wrapper for a first-contact chat request.
pub fn encode_encrypted_chat_request(
    request: &V2EncryptedChatRequest,
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    encode_bytes(&mut out, &request.encryption_pubkey)?;
    encode_bytes(&mut out, &request.encrypted_request)?;
    Ok(out)
}

/// Decode the encrypted outer wrapper for a first-contact chat request.
pub fn decode_encrypted_chat_request(data: &[u8]) -> Result<V2EncryptedChatRequest, ChatError> {
    let mut cursor = Cursor::new(data);
    let encryption_pubkey = cursor.read_bytes("encryption_pubkey")?;
    let encrypted_request = cursor.read_bytes("encrypted_request")?;
    cursor.finish()?;
    Ok(V2EncryptedChatRequest {
        encryption_pubkey,
        encrypted_request,
    })
}

/// Encode and seal request content V2 with a caller-provided fresh ephemeral
/// private key and unique nonce.
///
/// The output is `SCALE Data(ephemeral_public_key) || SCALE
/// Data(nonce || ciphertext || tag)`.
pub fn seal_chat_request_v2_with_nonce(
    ephemeral_private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    request: &V2ChatRequestV2,
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let plaintext = encode_chat_request_v2(request)?;
    let key = x25519_hkdf_sha256_key(ephemeral_private_key, peer_public_key)?;
    let encrypted_request = chacha20poly1305_encrypt_with_nonce(&key, &plaintext, nonce)?;
    encode_encrypted_chat_request(&V2EncryptedChatRequest {
        encryption_pubkey: x25519_public_key(ephemeral_private_key).to_vec(),
        encrypted_request,
    })
}

/// Encode and seal request content V2 with OS-generated ephemeral key material
/// and nonce.
#[cfg(feature = "std")]
pub fn seal_chat_request_v2(
    peer_public_key: &[u8; 32],
    request: &V2ChatRequestV2,
) -> Result<Vec<u8>, ChatError> {
    let ephemeral_private_key = random_bytes_32()?;
    seal_chat_request_v2_with_nonce(
        &ephemeral_private_key,
        peer_public_key,
        request,
        random_nonce()?,
    )
}

/// Open and decode a request-content-V2 first-contact wrapper.
pub fn open_chat_request_v2(
    static_private_key: &[u8; 32],
    encoded_wrapper: &[u8],
) -> Result<V2ChatRequestV2, ChatError> {
    let wrapper = decode_encrypted_chat_request(encoded_wrapper)?;
    let ephemeral_public_key: [u8; 32] =
        wrapper
            .encryption_pubkey
            .try_into()
            .map_err(|key: Vec<u8>| {
                ChatError::InvalidEncoding(format!(
                    "first-contact ephemeral public key must be 32 bytes, got {}",
                    key.len()
                ))
            })?;
    let key = x25519_hkdf_sha256_key(static_private_key, &ephemeral_public_key)?;
    let plaintext = chacha20poly1305_decrypt(&key, &wrapper.encrypted_request)?;
    decode_chat_request_v2(&plaintext)
}
/// Seal a first-contact request with explicit product, account, route, and
/// direction binding. The clear header is authenticated as AEAD context.
#[allow(clippy::too_many_arguments)]
pub fn seal_context_bound_chat_request_v2_with_nonce(
    ephemeral_private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    product_id: &str,
    sender_account_id: &[u8; 32],
    recipient_account_id: &[u8; 32],
    channel_id: &[u8; 32],
    request: &V2ChatRequestV2,
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    if request.message.content.identity_proof.identity_account_id != *sender_account_id {
        return Err(ChatError::InvalidEncoding(
            "context-bound invite sender does not match its identity proof".into(),
        ));
    }
    let ephemeral_public_key = x25519_public_key(ephemeral_private_key);
    let encrypted = x25519_context_bound_encrypt_with_nonce(
        ephemeral_private_key,
        peer_public_key,
        product_id,
        sender_account_id,
        recipient_account_id,
        channel_id,
        &encode_chat_request_v2(request)?,
        nonce,
    )?;
    let mut out = Vec::with_capacity(CONTEXT_BOUND_INVITE_MAGIC.len() + 128 + encrypted.len());
    out.extend_from_slice(CONTEXT_BOUND_INVITE_MAGIC);
    out.extend_from_slice(sender_account_id);
    out.extend_from_slice(recipient_account_id);
    out.extend_from_slice(channel_id);
    out.extend_from_slice(&ephemeral_public_key);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

/// Clear first-contact header. Fields remain untrusted until AEAD opening and
/// verification of the decrypted request identity and proof.
pub struct V2ContextBoundChatRequest<'a> {
    pub sender_account_id: [u8; 32],
    pub recipient_account_id: [u8; 32],
    pub channel_id: [u8; 32],
    pub ephemeral_public_key: [u8; 32],
    pub encrypted_request: &'a [u8],
}

/// Recognize the context-bound format family, including unsupported versions.
/// Invalid members of this family must never be retried as legacy invites.
pub fn is_context_bound_chat_request_v2(encoded_wrapper: &[u8]) -> bool {
    encoded_wrapper.starts_with(&CONTEXT_BOUND_INVITE_MAGIC[..5])
}

/// Decode and check the clear header without accessing identity secrets.
/// The caller must authenticate ciphertext and verify the decrypted sender.
pub fn decode_context_bound_chat_request_v2<'a>(
    expected_recipient_account_id: &[u8; 32],
    expected_channel_id: &[u8; 32],
    encoded_wrapper: &'a [u8],
) -> Result<V2ContextBoundChatRequest<'a>, ChatError> {
    const HEADER_LEN: usize = 8 + 32 + 32 + 32 + 32;
    if encoded_wrapper.len() < HEADER_LEN + 28
        || &encoded_wrapper[..CONTEXT_BOUND_INVITE_MAGIC.len()] != CONTEXT_BOUND_INVITE_MAGIC
    {
        return Err(ChatError::InvalidEncoding(
            "invalid context-bound invite header".into(),
        ));
    }
    let sender_account_id: [u8; 32] = encoded_wrapper[8..40]
        .try_into()
        .map_err(|_| ChatError::InvalidEncoding("invalid context-bound invite sender".into()))?;
    let recipient_account_id: [u8; 32] = encoded_wrapper[40..72]
        .try_into()
        .map_err(|_| ChatError::InvalidEncoding("invalid context-bound invite recipient".into()))?;
    let channel_id: [u8; 32] = encoded_wrapper[72..104]
        .try_into()
        .map_err(|_| ChatError::InvalidEncoding("invalid context-bound invite route".into()))?;
    let ephemeral_public_key: [u8; 32] =
        encoded_wrapper[104..HEADER_LEN].try_into().map_err(|_| {
            ChatError::InvalidEncoding("invalid context-bound invite ephemeral key".into())
        })?;
    if recipient_account_id != *expected_recipient_account_id || channel_id != *expected_channel_id
    {
        return Err(ChatError::InvalidEncoding(
            "context-bound invite recipient or route mismatch".into(),
        ));
    }
    Ok(V2ContextBoundChatRequest {
        sender_account_id,
        recipient_account_id,
        channel_id,
        ephemeral_public_key,
        encrypted_request: &encoded_wrapper[HEADER_LEN..],
    })
}

/// Open a context-bound first-contact request without legacy fallback.
pub fn open_context_bound_chat_request_v2(
    static_private_key: &[u8; 32],
    product_id: &str,
    expected_recipient_account_id: &[u8; 32],
    expected_channel_id: &[u8; 32],
    encoded_wrapper: &[u8],
) -> Result<V2ChatRequestV2, ChatError> {
    let V2ContextBoundChatRequest {
        sender_account_id,
        recipient_account_id,
        channel_id,
        ephemeral_public_key,
        encrypted_request,
    } = decode_context_bound_chat_request_v2(
        expected_recipient_account_id,
        expected_channel_id,
        encoded_wrapper,
    )?;
    let plaintext = x25519_context_bound_decrypt(
        static_private_key,
        &ephemeral_public_key,
        product_id,
        &sender_account_id,
        &recipient_account_id,
        &channel_id,
        encrypted_request,
    )?;
    let request = decode_chat_request_v2(&plaintext)?;
    if request.message.content.identity_proof.identity_account_id != sender_account_id {
        return Err(ChatError::InvalidEncoding(
            "context-bound invite identity proof does not match sender".into(),
        ));
    }
    Ok(request)
}

/// Encode the plaintext StatementData::request payload without applying AEAD.
pub fn encode_transport_request_plaintext(
    request_id: &str,
    messages: &[Vec<u8>],
) -> Result<Vec<u8>, ChatError> {
    let mut plaintext = Vec::new();
    plaintext.push(0);
    encode_string(&mut plaintext, request_id)?;
    encode_vec_of_bytes(&mut plaintext, messages)?;
    Ok(plaintext)
}

/// Encode the plaintext StatementData::response payload without applying AEAD.
pub fn encode_transport_response_plaintext(
    request_id: &str,
    response_code: u8,
) -> Result<Vec<u8>, ChatError> {
    let mut plaintext = Vec::new();
    plaintext.push(1);
    encode_string(&mut plaintext, request_id)?;
    plaintext.push(response_code);
    Ok(plaintext)
}

/// Encode bare MessageExchange::Request plaintext for the encryptedRequest
/// field of StatementData::MultiRequest.
pub fn encode_message_exchange_request_plaintext(
    request_id: &str,
    messages: &[Vec<u8>],
) -> Result<Vec<u8>, ChatError> {
    let mut plaintext = Vec::new();
    encode_string(&mut plaintext, request_id)?;
    encode_vec_of_bytes(&mut plaintext, messages)?;
    Ok(plaintext)
}

/// Decode bare MessageExchange::Request plaintext.
pub fn decode_message_exchange_request_plaintext(
    plaintext: &[u8],
) -> Result<V2MessageExchangeRequest, ChatError> {
    let mut cursor = Cursor::new(plaintext);
    let request_id = cursor.read_string("request_id")?;
    let messages = cursor.read_vec_of_bytes("messages")?;
    cursor.finish()?;
    Ok(V2MessageExchangeRequest {
        request_id,
        messages,
    })
}

/// Encode bare MessageExchange::Response plaintext for the encryptedResponse
/// field of StatementData::MultiResponse.
pub fn encode_message_exchange_response_plaintext(
    request_id: &str,
    response_code: u8,
) -> Result<Vec<u8>, ChatError> {
    let mut plaintext = Vec::new();
    encode_string(&mut plaintext, request_id)?;
    plaintext.push(response_code);
    Ok(plaintext)
}

/// Decode bare MessageExchange::Response plaintext.
pub fn decode_message_exchange_response_plaintext(
    plaintext: &[u8],
) -> Result<V2MessageExchangeResponse, ChatError> {
    let mut cursor = Cursor::new(plaintext);
    let request_id = cursor.read_string("request_id")?;
    let response_code = cursor.read_u8("response_code")?;
    cursor.finish()?;
    Ok(V2MessageExchangeResponse {
        request_id,
        response_code,
    })
}

/// Encode the plaintext StatementData::multirequest payload without applying AEAD.
pub fn encode_transport_multi_request_plaintext(
    request: &V2MultiDeviceRequest,
) -> Result<Vec<u8>, ChatError> {
    let mut plaintext = Vec::new();
    plaintext.push(2);
    encode_bytes(&mut plaintext, &request.encrypted_request)?;
    encode_request_device_infos(&mut plaintext, &request.devices_info)?;
    Ok(plaintext)
}

/// Encode the plaintext StatementData::multiresponse payload without applying AEAD.
pub fn encode_transport_multi_response_plaintext(
    response: &V2MultiDeviceResponse,
) -> Result<Vec<u8>, ChatError> {
    let mut plaintext = Vec::new();
    plaintext.push(3);
    encode_bytes(&mut plaintext, &response.encrypted_response)?;
    encode_request_device_infos(&mut plaintext, &response.devices_info)?;
    Ok(plaintext)
}

/// Encode and encrypt an ongoing v2 statement-store request payload with OS
/// randomness. Guests must use [`encode_transport_request_with_nonce`].
#[cfg(feature = "std")]
pub fn encode_transport_request(
    aead_key: &[u8; 32],
    request_id: &str,
    messages: &[Vec<u8>],
) -> Result<Vec<u8>, ChatError> {
    encode_transport_request_with_nonce(aead_key, request_id, messages, random_nonce()?)
}

/// Encode and encrypt a request with a caller-supplied unique nonce.
pub fn encode_transport_request_with_nonce(
    aead_key: &[u8; 32],
    request_id: &str,
    messages: &[Vec<u8>],
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let plaintext = encode_transport_request_plaintext(request_id, messages)?;
    chacha20poly1305_encrypt_with_nonce(aead_key, &plaintext, nonce)
}

/// Encode and encrypt an ongoing v2 response with OS randomness. Guests must
/// use [`encode_transport_response_with_nonce`].
#[cfg(feature = "std")]
pub fn encode_transport_response(
    aead_key: &[u8; 32],
    request_id: &str,
    response_code: u8,
) -> Result<Vec<u8>, ChatError> {
    encode_transport_response_with_nonce(aead_key, request_id, response_code, random_nonce()?)
}

/// Encode and encrypt a response with a caller-supplied unique nonce.
pub fn encode_transport_response_with_nonce(
    aead_key: &[u8; 32],
    request_id: &str,
    response_code: u8,
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let plaintext = encode_transport_response_plaintext(request_id, response_code)?;
    chacha20poly1305_encrypt_with_nonce(aead_key, &plaintext, nonce)
}

/// Encode and encrypt StatementData::multirequest with OS randomness.
#[cfg(feature = "std")]
pub fn encode_transport_multi_request(
    aead_key: &[u8; 32],
    request: &V2MultiDeviceRequest,
) -> Result<Vec<u8>, ChatError> {
    encode_transport_multi_request_with_nonce(aead_key, request, random_nonce()?)
}

/// Encode and encrypt StatementData::multiresponse with OS randomness.
#[cfg(feature = "std")]
pub fn encode_transport_multi_response(
    aead_key: &[u8; 32],
    response: &V2MultiDeviceResponse,
) -> Result<Vec<u8>, ChatError> {
    encode_transport_multi_response_with_nonce(aead_key, response, random_nonce()?)
}

/// Wrap a one-shot key with an OS-random nonce.
#[cfg(feature = "std")]
pub fn wrap_multi_device_key(
    own_private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    one_shot_key: &[u8; 32],
) -> Result<Vec<u8>, ChatError> {
    wrap_multi_device_key_with_nonce(
        own_private_key,
        peer_public_key,
        one_shot_key,
        random_nonce()?,
    )
}

/// Encrypt an inner multi-device payload with an OS-random nonce.
#[cfg(feature = "std")]
pub fn encrypt_multi_device_payload(
    one_shot_key: &[u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>, ChatError> {
    encrypt_multi_device_payload_with_nonce(one_shot_key, plaintext, random_nonce()?)
}

/// Encode and encrypt StatementData::multirequest with a caller-supplied nonce.
pub fn encode_transport_multi_request_with_nonce(
    aead_key: &[u8; 32],
    request: &V2MultiDeviceRequest,
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let plaintext = encode_transport_multi_request_plaintext(request)?;
    chacha20poly1305_encrypt_with_nonce(aead_key, &plaintext, nonce)
}

/// Encode and encrypt StatementData::multiresponse with a caller-supplied nonce.
pub fn encode_transport_multi_response_with_nonce(
    aead_key: &[u8; 32],
    response: &V2MultiDeviceResponse,
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let plaintext = encode_transport_multi_response_plaintext(response)?;
    chacha20poly1305_encrypt_with_nonce(aead_key, &plaintext, nonce)
}

/// Encrypt a one-shot key for one recipient device using the X25519-derived
/// CryptoKit-compatible key and a caller-supplied unique nonce.
pub fn wrap_multi_device_key_with_nonce(
    own_private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    one_shot_key: &[u8; 32],
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    let wrapping_key = x25519_hkdf_sha256_key(own_private_key, peer_public_key)?;
    chacha20poly1305_encrypt_with_nonce(&wrapping_key, one_shot_key, nonce)
}

/// Unwrap a one-shot key received from a peer device.
pub fn unwrap_multi_device_key(
    own_private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
    encrypted_key: &[u8],
) -> Result<[u8; 32], ChatError> {
    let wrapping_key = x25519_hkdf_sha256_key(own_private_key, peer_public_key)?;
    let plaintext = chacha20poly1305_decrypt(&wrapping_key, encrypted_key)?;
    plaintext.try_into().map_err(|plaintext: Vec<u8>| {
        ChatError::InvalidEncoding(format!(
            "unwrapped multi-device key must be 32 bytes, got {}",
            plaintext.len()
        ))
    })
}

/// Encrypt an inner multi-device request/response using its one-shot key and a
/// caller-supplied unique nonce.
pub fn encrypt_multi_device_payload_with_nonce(
    one_shot_key: &[u8; 32],
    plaintext: &[u8],
    nonce: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    chacha20poly1305_encrypt_with_nonce(one_shot_key, plaintext, nonce)
}

/// Decrypt an inner multi-device request/response.
pub fn decrypt_multi_device_payload(
    one_shot_key: &[u8; 32],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ChatError> {
    chacha20poly1305_decrypt(one_shot_key, ciphertext)
}

/// Decode and decrypt an ongoing v2 statement-store transport payload.
pub fn decode_transport(
    data: &[u8],
    aead_key: &[u8; 32],
) -> Result<V2StatementTransportData, ChatError> {
    let plaintext = match chacha20poly1305_decrypt(aead_key, data) {
        Ok(plaintext) => plaintext,
        Err(raw_error) => {
            let mut outer = Cursor::new(data);
            let encrypted = outer.read_bytes("encrypted_transport")?;
            outer.finish()?;
            chacha20poly1305_decrypt(aead_key, &encrypted).map_err(|_| raw_error)?
        }
    };
    decode_transport_plaintext(&plaintext)
}

/// Decode plaintext StatementData after a Host has opened the identity route.
pub fn decode_transport_plaintext(plaintext: &[u8]) -> Result<V2StatementTransportData, ChatError> {
    let mut cursor = Cursor::new(plaintext);
    let kind = cursor.read_u8("transport_kind")?;
    let decoded = match kind {
        0 => {
            let request_id = cursor.read_string("request_id")?;
            let messages = cursor.read_vec_of_bytes("messages")?;
            cursor.finish()?;
            V2StatementTransportData::Request {
                request_id,
                messages,
            }
        }
        1 => {
            let request_id = cursor.read_string("request_id")?;
            let response_code = cursor.read_u8("response_code")?;
            cursor.finish()?;
            V2StatementTransportData::Response {
                request_id,
                response_code,
            }
        }
        2 => {
            let encrypted_request = cursor.read_bytes("encrypted_request")?;
            let devices_info = cursor.read_request_device_infos("devices_info")?;
            cursor.finish()?;
            V2StatementTransportData::MultiRequest(V2MultiDeviceRequest {
                encrypted_request,
                devices_info,
            })
        }
        3 => {
            let encrypted_response = cursor.read_bytes("encrypted_response")?;
            let devices_info = cursor.read_request_device_infos("devices_info")?;
            cursor.finish()?;
            V2StatementTransportData::MultiResponse(V2MultiDeviceResponse {
                encrypted_response,
                devices_info,
            })
        }
        value => {
            return Err(ChatError::InvalidEncoding(format!(
                "unsupported v2 transport kind {value}"
            )));
        }
    };
    Ok(decoded)
}

fn encode_message(
    message_id: &str,
    timestamp: u64,
    encode_content: impl FnOnce(&mut Vec<u8>) -> Result<(), ChatError>,
) -> Result<Vec<u8>, ChatError> {
    let mut out = Vec::new();
    encode_string(&mut out, message_id)?;
    out.extend_from_slice(&timestamp.to_le_bytes());
    out.push(0); // VersionedChatMessage.V1
    encode_content(&mut out)?;
    Ok(out)
}

fn encode_string(out: &mut Vec<u8>, value: &str) -> Result<(), ChatError> {
    let len = u32::try_from(value.len()).map_err(|_| {
        ChatError::InvalidEncoding("string is too large for SCALE Compact<u32>".into())
    })?;
    out.extend_from_slice(&encode_compact_u32(len));
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn encode_bytes(out: &mut Vec<u8>, value: &[u8]) -> Result<(), ChatError> {
    let len = u32::try_from(value.len()).map_err(|_| {
        ChatError::InvalidEncoding("byte array is too large for SCALE Compact<u32>".into())
    })?;
    out.extend_from_slice(&encode_compact_u32(len));
    out.extend_from_slice(value);
    Ok(())
}

fn encode_balance(out: &mut Vec<u8>, value: &str) -> Result<(), ChatError> {
    let value = value
        .parse::<u128>()
        .map_err(|_| ChatError::InvalidEncoding(format!("invalid balance value: {value}")))?;
    out.extend_from_slice(&encode_compact_u128(value));
    Ok(())
}

fn encode_compact_u128(value: u128) -> Vec<u8> {
    if value < 1 << 6 {
        vec![(value as u8) << 2]
    } else if value < 1 << 14 {
        (((value as u16) << 2) | 0b01).to_le_bytes().to_vec()
    } else if value < 1 << 30 {
        (((value as u32) << 2) | 0b10).to_le_bytes().to_vec()
    } else {
        let mut bytes = value.to_le_bytes().to_vec();
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        let header = (((bytes.len() - 4) as u8) << 2) | 0b11;
        let mut out = Vec::with_capacity(1 + bytes.len());
        out.push(header);
        out.extend_from_slice(&bytes);
        out
    }
}

fn push_platform_index(platform: V2PushPlatform) -> u8 {
    match platform {
        V2PushPlatform::Android => 0,
        V2PushPlatform::Ios => 1,
        V2PushPlatform::IosVoip => 2,
    }
}

fn decode_push_platform(value: u8) -> Result<V2PushPlatform, ChatError> {
    match value {
        0 => Ok(V2PushPlatform::Android),
        1 => Ok(V2PushPlatform::Ios),
        2 => Ok(V2PushPlatform::IosVoip),
        value => Err(ChatError::InvalidEncoding(format!(
            "unsupported push platform {value}"
        ))),
    }
}

fn encode_rich_text(
    out: &mut Vec<u8>,
    text: Option<&str>,
    attachments: Option<&[V2FileVariant]>,
) -> Result<(), ChatError> {
    match text {
        Some(t) => {
            out.push(1); // text = Some
            encode_string(out, t)?;
        }
        None => out.push(0), // text = None
    }
    match attachments {
        None => out.push(0),
        Some(files) => {
            let count = u32::try_from(files.len())
                .map_err(|_| ChatError::InvalidEncoding("too many attachments".into()))?;
            out.push(1);
            out.extend_from_slice(&encode_compact_u32(count));
            for file in files {
                encode_file(out, file)?;
            }
        }
    }
    Ok(())
}

fn validate_file_node(node: &V2NodeEndpoint) -> Result<(), ChatError> {
    let V2NodeEndpoint::WssUrl(url) = node;
    let invalid = || ChatError::InvalidEncoding("invalid attachment WSS endpoint".into());
    let (scheme, rest) = url.split_once("://").ok_or_else(invalid)?;
    if !scheme.eq_ignore_ascii_case("wss")
        || url
            .bytes()
            .any(|byte| byte <= b' ' || byte == 0x7f || byte == b'\\')
        || rest.contains('#')
    {
        return Err(invalid());
    }
    let authority = rest.split(['/', '?']).next().ok_or_else(invalid)?;
    if authority.is_empty() || authority.contains('@') {
        return Err(invalid());
    }
    let port = if let Some(ipv6) = authority.strip_prefix('[') {
        let (address, suffix) = ipv6.split_once(']').ok_or_else(invalid)?;
        address
            .parse::<core::net::Ipv6Addr>()
            .map_err(|_| invalid())?;
        if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':').ok_or_else(invalid)?)
        }
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty()
            || host.len() > 253
            || !host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        {
            return Err(invalid());
        }
        if host
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            host.parse::<core::net::Ipv4Addr>().map_err(|_| invalid())?;
        }
        port
    };
    if let Some(port) = port
        && (port.is_empty()
            || !port.bytes().all(|byte| byte.is_ascii_digit())
            || port.parse::<u16>().is_err())
    {
        return Err(invalid());
    }
    Ok(())
}

fn encode_file(out: &mut Vec<u8>, file: &V2FileVariant) -> Result<(), ChatError> {
    let V2FileVariant::P2pMixnet(file) = file;
    if file.identifier.len() != 32 || file.claim_ticket.len() != 32 {
        return Err(ChatError::InvalidEncoding(
            "attachment identifier and claim ticket must be exactly 32 bytes".into(),
        ));
    }
    validate_file_node(&file.node)?;
    out.push(0); // FileVariant::P2pMixnet
    encode_bytes(out, &file.identifier)?;
    encode_bytes(out, &file.claim_ticket)?;
    let V2NodeEndpoint::WssUrl(url) = &file.node;
    out.push(0); // NodeEndpoint::WssUrl
    encode_string(out, url)?;
    match &file.meta {
        V2FileMeta::General(general) => {
            out.push(0);
            encode_general_file_meta(out, general)?;
        }
        V2FileMeta::Image(image) => {
            out.push(1);
            encode_general_file_meta(out, &image.general)?;
            out.extend_from_slice(&image.width.to_le_bytes());
            out.extend_from_slice(&image.height.to_le_bytes());
            encode_thumbnail(out, image.thumbnail.as_deref())?;
        }
        V2FileMeta::Video(video) => {
            out.push(2);
            encode_general_file_meta(out, &video.general)?;
            out.extend_from_slice(&video.duration.to_le_bytes());
            encode_thumbnail(out, video.thumbnail.as_deref())?;
        }
    }
    Ok(())
}

fn encode_general_file_meta(
    out: &mut Vec<u8>,
    general: &V2GeneralFileMeta,
) -> Result<(), ChatError> {
    encode_string(out, &general.mime_type)?;
    out.extend_from_slice(&general.file_size.to_le_bytes());
    Ok(())
}

fn encode_thumbnail(out: &mut Vec<u8>, thumbnail: Option<&[u8]>) -> Result<(), ChatError> {
    match thumbnail {
        None => out.push(0),
        Some(bytes) => {
            out.push(1);
            encode_bytes(out, bytes)?;
        }
    }
    Ok(())
}

fn decode_file(cursor: &mut Cursor<'_>) -> Result<V2FileVariant, ChatError> {
    let variant = cursor.read_u8("file_variant")?;
    if variant != 0 {
        return Err(ChatError::InvalidEncoding(format!(
            "unsupported file variant {variant}"
        )));
    }
    let identifier = decode_file_key(cursor, "file_identifier")?;
    let mut claim_ticket = zeroize::Zeroizing::new(decode_file_key(cursor, "file_claim_ticket")?);
    let node = match cursor.read_u8("file_node_endpoint")? {
        0 => V2NodeEndpoint::WssUrl(cursor.read_string("file_wss_url")?),
        value => {
            return Err(ChatError::InvalidEncoding(format!(
                "unsupported attachment node endpoint {value}"
            )));
        }
    };
    validate_file_node(&node)?;
    let meta_index = cursor.read_u8("file_meta")?;
    if meta_index > 2 {
        return Err(ChatError::InvalidEncoding(format!(
            "unsupported file metadata {meta_index}"
        )));
    }
    let general = V2GeneralFileMeta {
        mime_type: cursor.read_string("file_mime_type")?,
        file_size: cursor.read_u32("file_size")?,
    };
    let meta = match meta_index {
        0 => V2FileMeta::General(general),
        1 => V2FileMeta::Image(V2ImageFileMeta {
            general,
            width: cursor.read_u32("image_width")?,
            height: cursor.read_u32("image_height")?,
            thumbnail: decode_thumbnail(cursor)?,
        }),
        2 => V2FileMeta::Video(V2VideoFileMeta {
            general,
            duration: cursor.read_u32("video_duration")?,
            thumbnail: decode_thumbnail(cursor)?,
        }),
        _ => unreachable!(),
    };
    Ok(V2FileVariant::P2pMixnet(V2P2pMixnetFile {
        identifier,
        claim_ticket: core::mem::take(&mut *claim_ticket),
        node,
        meta,
    }))
}

fn decode_file_key(cursor: &mut Cursor<'_>, field: &str) -> Result<Vec<u8>, ChatError> {
    let (len, consumed) =
        decode_compact_u32(&cursor.data[cursor.offset..]).map_err(ChatError::InvalidEncoding)?;
    cursor.offset += consumed;
    if len != 32 {
        return Err(ChatError::InvalidEncoding(format!(
            "{field} must be exactly 32 bytes"
        )));
    }
    Ok(cursor.read_exact(32, field)?.to_vec())
}

fn decode_thumbnail(cursor: &mut Cursor<'_>) -> Result<Option<Vec<u8>>, ChatError> {
    match cursor.read_u8("thumbnail_option")? {
        0 => Ok(None),
        1 => Ok(Some(cursor.read_bytes("thumbnail")?)),
        value => Err(ChatError::InvalidEncoding(format!(
            "invalid SCALE option for thumbnail: {value}"
        ))),
    }
}

fn encode_optional_push_token(
    out: &mut Vec<u8>,
    push_token: Option<&V2PushToken>,
) -> Result<(), ChatError> {
    match push_token {
        Some(push_token) => {
            out.push(1);
            encode_bytes(out, &push_token.token)?;
            out.push(match push_token.platform {
                V2PushPlatform::Android => 0,
                V2PushPlatform::Ios => 1,
                V2PushPlatform::IosVoip => 2,
            });
        }
        None => out.push(0),
    }
    Ok(())
}

fn encode_optional_rich_text(
    out: &mut Vec<u8>,
    welcome_text: Option<&str>,
) -> Result<(), ChatError> {
    match welcome_text {
        Some(text) => {
            out.push(1); // Some(RichText)
            out.push(1); // RichText.text = Some
            encode_string(out, text)?;
            out.push(0); // RichText.attachments = None
        }
        None => out.push(0),
    }
    Ok(())
}

fn encode_vec_of_bytes(out: &mut Vec<u8>, items: &[Vec<u8>]) -> Result<(), ChatError> {
    let len = u32::try_from(items.len()).map_err(|_| {
        ChatError::InvalidEncoding("messages vector is too large for SCALE Compact<u32>".into())
    })?;
    out.extend_from_slice(&encode_compact_u32(len));
    for item in items {
        encode_bytes(out, item)?;
    }
    Ok(())
}

fn encode_request_device_infos(
    out: &mut Vec<u8>,
    devices: &[V2RequestDeviceInfo],
) -> Result<(), ChatError> {
    let len = u32::try_from(devices.len()).map_err(|_| {
        ChatError::InvalidEncoding("devices vector is too large for SCALE Compact<u32>".into())
    })?;
    out.extend_from_slice(&encode_compact_u32(len));
    for device in devices {
        out.extend_from_slice(&device.statement_account_id);
        encode_bytes(out, &device.encrypted_key)?;
    }
    Ok(())
}

fn decode_chat_request_message_cursor(
    cursor: &mut Cursor<'_>,
) -> Result<V2ChatRequestMessage, ChatError> {
    let message_id = cursor.read_string("message_id")?;
    let timestamp = cursor.read_u64("timestamp")?;
    let version_index = cursor.read_u8("version_index")?;
    if version_index != 0 {
        return Err(ChatError::InvalidEncoding(format!(
            "unsupported v2 chat request version index {version_index}"
        )));
    }

    let push_token = decode_optional_push_token(cursor)?;
    let welcome_text = decode_optional_rich_text(cursor)?;

    Ok(V2ChatRequestMessage {
        message_id,
        timestamp,
        push_token,
        welcome_text,
    })
}

fn decode_chat_request_message_v2_cursor(
    cursor: &mut Cursor<'_>,
) -> Result<V2ChatRequestMessageV2, ChatError> {
    let message_id = cursor.read_string("message_id")?;
    let timestamp = cursor.read_u64("timestamp")?;
    let version_index = cursor.read_u8("version_index")?;
    if version_index != 1 {
        return Err(ChatError::InvalidEncoding(format!(
            "expected chat request content version index 1, got {version_index}"
        )));
    }
    let identity_account_id = cursor.read_array_32("identity_account_id")?;
    let proof = cursor.read_array_32("identity_proof")?;
    let device_enc_pub_key = cursor.read_array_32("device_enc_pub_key")?;
    let push_token = decode_optional_push_token(cursor)?;
    let welcome_text = decode_optional_rich_text(cursor)?;
    Ok(V2ChatRequestMessageV2 {
        message_id,
        timestamp,
        content: V2ChatRequestContentV2 {
            identity_proof: V2ChatRequestIdentityProof {
                identity_account_id,
                proof,
            },
            device_enc_pub_key,
            push_token,
            welcome_text,
        },
    })
}

fn validate_chat_request_proof(proof: &V2ChatRequestProof) -> Result<(), ChatError> {
    if proof.signature.len() != 64 {
        return Err(ChatError::InvalidEncoding(format!(
            "chat request proof signature must be 64 bytes, got {}",
            proof.signature.len()
        )));
    }
    if proof.signer.len() != 32 {
        return Err(ChatError::InvalidEncoding(format!(
            "chat request proof signer must be 32 bytes, got {}",
            proof.signer.len()
        )));
    }
    Ok(())
}

fn decode_chat_request_proof(cursor: &mut Cursor<'_>) -> Result<V2ChatRequestProof, ChatError> {
    let proof_index = cursor.read_u8("proof_index")?;
    if proof_index != 0 {
        return Err(ChatError::InvalidEncoding(format!(
            "unsupported chat request proof index {proof_index}"
        )));
    }
    let signature = cursor.read_exact(64, "proof_signature")?.to_vec();
    let signer = cursor.read_exact(32, "proof_signer")?.to_vec();
    Ok(V2ChatRequestProof { signature, signer })
}

fn decode_optional_push_token(cursor: &mut Cursor<'_>) -> Result<Option<V2PushToken>, ChatError> {
    match cursor.read_u8("push_token_option")? {
        0 => Ok(None),
        1 => {
            let token = cursor.read_bytes("push_token")?;
            let platform = match cursor.read_u8("push_platform")? {
                0 => V2PushPlatform::Android,
                1 => V2PushPlatform::Ios,
                2 => V2PushPlatform::IosVoip,
                value => {
                    return Err(ChatError::InvalidEncoding(format!(
                        "unsupported push platform {value}"
                    )));
                }
            };
            Ok(Some(V2PushToken { token, platform }))
        }
        value => Err(ChatError::InvalidEncoding(format!(
            "invalid SCALE option for push token: {value}"
        ))),
    }
}

/// Decode a RichText value from the cursor (non-optional outer wrapper).
///
/// RichText = Option<String> text + Option<Attachments> attachments.
fn decode_rich_text(
    cursor: &mut Cursor<'_>,
) -> Result<(Option<String>, Option<Vec<V2FileVariant>>), ChatError> {
    let text = match cursor.read_u8("rich_text_text_option")? {
        0 => None,
        1 => Some(cursor.read_string("rich_text_text")?),
        value => {
            return Err(ChatError::InvalidEncoding(format!(
                "invalid SCALE option for rich text: {value}"
            )));
        }
    };

    let attachments = match cursor.read_u8("rich_text_attachments_option")? {
        0 => None,
        1 => {
            let (count, consumed) = decode_compact_u32(&cursor.data[cursor.offset..])
                .map_err(ChatError::InvalidEncoding)?;
            cursor.offset += consumed;
            let count = count as usize;
            // Even with empty URL/MIME strings, a native file needs 75 bytes.
            // Bound the collection before reserving from an untrusted count.
            if count > cursor.data.len().saturating_sub(cursor.offset) / 75 {
                return Err(ChatError::InvalidEncoding(
                    "attachment count exceeds remaining input".into(),
                ));
            }
            let mut files = Vec::with_capacity(count);
            for _ in 0..count {
                files.push(decode_file(cursor)?);
            }
            Some(files)
        }
        value => {
            return Err(ChatError::InvalidEncoding(format!(
                "invalid SCALE option for rich text attachments: {value}"
            )));
        }
    };
    Ok((text, attachments))
}

fn decode_optional_rich_text(cursor: &mut Cursor<'_>) -> Result<Option<String>, ChatError> {
    match cursor.read_u8("welcome_option")? {
        0 => Ok(None),
        1 => {
            let text = match cursor.read_u8("welcome_text_option")? {
                0 => None,
                1 => Some(cursor.read_string("welcome_text")?),
                value => {
                    return Err(ChatError::InvalidEncoding(format!(
                        "invalid SCALE option for rich text text: {value}"
                    )));
                }
            };

            match cursor.read_u8("welcome_attachments_option")? {
                0 => Ok(text),
                1 => Err(ChatError::InvalidEncoding(
                    "welcome message attachments cannot be represented".into(),
                )),
                value => Err(ChatError::InvalidEncoding(format!(
                    "invalid SCALE option for rich text attachments: {value}"
                ))),
            }
        }
        value => Err(ChatError::InvalidEncoding(format!(
            "invalid SCALE option for welcome message: {value}"
        ))),
    }
}

#[cfg(feature = "std")]
fn random_bytes_32() -> Result<[u8; 32], ChatError> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|_| ChatError::KeyDerivationFailed("OS key generation failed".into()))?;
    Ok(bytes)
}

#[cfg(feature = "std")]
fn random_nonce() -> Result<[u8; 12], ChatError> {
    let mut nonce = [0; 12];
    getrandom::getrandom(&mut nonce)
        .map_err(|_| ChatError::KeyDerivationFailed("OS nonce generation failed".into()))?;
    Ok(nonce)
}

fn chacha20poly1305_encrypt_with_nonce(
    key: &[u8; 32],
    plaintext: &[u8],
    nonce_bytes: [u8; 12],
) -> Result<Vec<u8>, ChatError> {
    chacha20poly1305_encrypt_with_nonce_and_aad(key, plaintext, nonce_bytes, &[])
}

fn chacha20poly1305_encrypt_with_nonce_and_aad(
    key: &[u8; 32],
    plaintext: &[u8],
    nonce_bytes: [u8; 12],
    aad: &[u8],
) -> Result<Vec<u8>, ChatError> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| ChatError::InvalidEncoding("ChaCha20-Poly1305 encryption failed".into()))?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn chacha20poly1305_decrypt(key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, ChatError> {
    chacha20poly1305_decrypt_with_aad(key, ciphertext, &[])
}

fn chacha20poly1305_decrypt_with_aad(
    key: &[u8; 32],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, ChatError> {
    if ciphertext.len() < 28 {
        return Err(ChatError::InvalidEncoding(format!(
            "ciphertext too short: {} bytes",
            ciphertext.len()
        )));
    }

    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(&ciphertext[..12]);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &ciphertext[12..],
                aad,
            },
        )
        .map_err(|_| ChatError::InvalidEncoding("ChaCha20-Poly1305 decryption failed".into()))
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_u8(&mut self, field: &str) -> Result<u8, ChatError> {
        let value = *self.data.get(self.offset).ok_or_else(|| {
            ChatError::InvalidEncoding(format!("v2 chat message truncated at {field}"))
        })?;
        self.offset += 1;
        Ok(value)
    }

    #[cfg(feature = "std")]
    fn read_u16(&mut self, field: &str) -> Result<u16, ChatError> {
        let bytes = self.read_exact(2, field)?;
        Ok(u16::from_le_bytes(bytes.try_into().map_err(|_| {
            ChatError::InvalidEncoding(format!("v2 chat message invalid u16 for {field}"))
        })?))
    }

    fn read_u32(&mut self, field: &str) -> Result<u32, ChatError> {
        let bytes = self.read_exact(4, field)?;
        Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| {
            ChatError::InvalidEncoding(format!("v2 chat message invalid u32 for {field}"))
        })?))
    }

    fn read_compact_u128(&mut self, field: &str) -> Result<u128, ChatError> {
        let first = self.read_u8(field)?;
        let mode = first & 0b11;
        let non_canonical =
            || ChatError::InvalidEncoding(format!("{field}: non-canonical compact integer"));
        let value = match mode {
            0b00 => u128::from(first >> 2),
            0b01 => {
                let next = self.read_exact(1, field)?[0];
                let value = u128::from(u16::from_le_bytes([first, next]) >> 2);
                if value < 1 << 6 {
                    return Err(non_canonical());
                }
                value
            }
            0b10 => {
                let rest = self.read_exact(3, field)?;
                let value = u128::from(u32::from_le_bytes([first, rest[0], rest[1], rest[2]]) >> 2);
                if value < 1 << 14 {
                    return Err(non_canonical());
                }
                value
            }
            0b11 => {
                let len = usize::from(first >> 2) + 4;
                if len > 16 {
                    return Err(ChatError::InvalidEncoding(format!(
                        "{field}: compact integer exceeds u128"
                    )));
                }
                let bytes = self.read_exact(len, field)?;
                if bytes[len - 1] == 0 {
                    return Err(non_canonical());
                }
                let mut padded = [0_u8; 16];
                padded[..len].copy_from_slice(bytes);
                let value = u128::from_le_bytes(padded);
                if value < 1 << 30 {
                    return Err(non_canonical());
                }
                value
            }
            _ => unreachable!(),
        };
        Ok(value)
    }

    fn read_u64(&mut self, field: &str) -> Result<u64, ChatError> {
        let bytes = self.read_exact(8, field)?;
        Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
            ChatError::InvalidEncoding(format!("v2 chat message invalid u64 for {field}"))
        })?))
    }

    fn read_array_32(&mut self, field: &str) -> Result<[u8; 32], ChatError> {
        self.read_exact(32, field)?
            .try_into()
            .map_err(|_| ChatError::InvalidEncoding(format!("invalid 32-byte field {field}")))
    }

    fn read_balance(&mut self, field: &str) -> Result<String, ChatError> {
        self.read_compact_u128(field).map(|value| value.to_string())
    }

    fn read_string(&mut self, field: &str) -> Result<String, ChatError> {
        let (len, consumed) = decode_compact_u32(&self.data[self.offset..])
            .map_err(|e| ChatError::InvalidEncoding(format!("{field}: {e}")))?;
        self.offset += consumed;
        let bytes = self.read_exact(len as usize, field)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| ChatError::InvalidEncoding(format!("{field}: invalid utf-8: {e}")))
    }

    fn read_exact(&mut self, len: usize, field: &str) -> Result<&'a [u8], ChatError> {
        let end = self.offset.checked_add(len).ok_or_else(|| {
            ChatError::InvalidEncoding(format!("v2 chat message length overflow at {field}"))
        })?;
        if end > self.data.len() {
            return Err(ChatError::InvalidEncoding(format!(
                "v2 chat message truncated at {field}"
            )));
        }
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn read_bytes(&mut self, field: &str) -> Result<Vec<u8>, ChatError> {
        let (len, consumed) = decode_compact_u32(&self.data[self.offset..])
            .map_err(|e| ChatError::InvalidEncoding(format!("{field}: {e}")))?;
        self.offset += consumed;
        Ok(self.read_exact(len as usize, field)?.to_vec())
    }

    fn read_vec_of_bytes(&mut self, field: &str) -> Result<Vec<Vec<u8>>, ChatError> {
        let (len, consumed) = decode_compact_u32(&self.data[self.offset..])
            .map_err(|e| ChatError::InvalidEncoding(format!("{field}: {e}")))?;
        self.offset += consumed;
        let len = len as usize;
        if len > self.data.len().saturating_sub(self.offset) {
            return Err(ChatError::InvalidEncoding(format!(
                "{field}: item count exceeds remaining input"
            )));
        }
        let mut out = Vec::with_capacity(len);
        for index in 0..len {
            out.push(self.read_bytes(&format!("{field}[{index}]"))?);
        }
        Ok(out)
    }

    fn read_request_device_infos(
        &mut self,
        field: &str,
    ) -> Result<Vec<V2RequestDeviceInfo>, ChatError> {
        let (len, consumed) = decode_compact_u32(&self.data[self.offset..])
            .map_err(|error| ChatError::InvalidEncoding(format!("{field}: {error}")))?;
        self.offset += consumed;
        let len = len as usize;
        if len > self.data.len().saturating_sub(self.offset) / 33 {
            return Err(ChatError::InvalidEncoding(format!(
                "{field}: device count exceeds remaining input"
            )));
        }
        let mut devices = Vec::with_capacity(len);
        for index in 0..len {
            devices.push(V2RequestDeviceInfo {
                statement_account_id: self
                    .read_array_32(&format!("{field}[{index}].statement_account_id"))?,
                encrypted_key: self.read_bytes(&format!("{field}[{index}].encrypted_key"))?,
            });
        }
        Ok(devices)
    }

    fn finish(&self) -> Result<(), ChatError> {
        if self.offset != self.data.len() {
            return Err(ChatError::InvalidEncoding(format!(
                "v2 chat message has {} trailing bytes",
                self.data.len() - self.offset
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_array_32(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64);
        let mut out = [0_u8; 32];
        for (index, byte) in out.iter_mut().enumerate() {
            let offset = index * 2;
            *byte = u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap();
        }
        out
    }

    #[test]
    fn chacha20poly1305_matches_rfc_8439_vector() {
        let key = hex_array_32("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        let nonce: [u8; 12] = hex::decode("070000004041424344454647")
            .unwrap()
            .try_into()
            .unwrap();
        let plaintext = hex::decode(concat!(
            "4c616469657320616e642047656e746c656d656e206f662074686520636c617373",
            "206f66202739393a204966204920636f756c64206f6666657220796f75206f6e6c",
            "79206f6e652074697020666f7220746865206675747572652c2073756e7363726565",
            "6e20776f756c642062652069742e"
        ))
        .unwrap();
        let aad = hex::decode("50515253c0c1c2c3c4c5c6c7").unwrap();
        let expected = hex::decode(concat!(
            "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63",
            "dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692d",
            "dbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4d",
            "ef08e4b7a9de576d26586cec64b6116",
            "1ae10b594f09e26a7e902ecbd0600691"
        ))
        .unwrap();

        use chacha20poly1305::aead::Payload;
        let cipher = ChaCha20Poly1305::new((&key).into());
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .unwrap();
        assert_eq!(encrypted, expected);
        assert_eq!(
            cipher
                .decrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &encrypted,
                        aad: &aad,
                    },
                )
                .unwrap(),
            plaintext
        );
    }

    #[test]
    fn day_from_unix_uses_v2_protocol_epoch() {
        assert_eq!(chat_request_day_from_unix(PROTOCOL_EPOCH_SECONDS - 1), None);
        assert_eq!(chat_request_day_from_unix(PROTOCOL_EPOCH_SECONDS), Some(0));
        assert_eq!(
            chat_request_day_from_unix(PROTOCOL_EPOCH_SECONDS + SECONDS_IN_DAY * 7 + 12),
            Some(7)
        );
    }

    #[test]
    fn full_topic_matches_ios_v2_data_layout() {
        let account = [0x11; 32];
        let mut expected_input = Vec::new();
        expected_input.extend_from_slice(&encode_compact_u32(CHAT_REQUEST_CONTEXT.len() as u32));
        expected_input.extend_from_slice(CHAT_REQUEST_CONTEXT);
        expected_input.extend_from_slice(&encode_compact_u32(account.len() as u32));
        expected_input.extend_from_slice(&account);
        assert_eq!(
            chat_request_full_topic(&account),
            blake2b_256(&expected_input)
        );
    }

    #[test]
    fn day_topic_appends_little_endian_u64_day() {
        let account = [0x22; 32];
        let day = 42_u64;
        let mut expected_input = Vec::new();
        expected_input.extend_from_slice(&encode_compact_u32(CHAT_REQUEST_CONTEXT.len() as u32));
        expected_input.extend_from_slice(CHAT_REQUEST_CONTEXT);
        expected_input.extend_from_slice(&encode_compact_u32(account.len() as u32));
        expected_input.extend_from_slice(&account);
        expected_input.extend_from_slice(&day.to_le_bytes());
        assert_eq!(
            chat_request_day_topic(&account, day),
            blake2b_256(&expected_input)
        );
    }

    #[test]
    fn full_topic_matches_ios_v2_observed_topic() {
        let account =
            hex_array_32("e6f8b1d6f1c8fde666469b9662d3d0925b21085f876722e428b1226d78ae1301");
        assert_eq!(
            chat_request_full_topic(&account),
            hex_array_32("463725e892e8c663bb531e8ed8ecf4b4e7e4a198ddfbce76a646d515dab1c5b2")
        );
    }

    #[test]
    fn session_id_params_match_v2_ordering() {
        let requester = [0xAA; 32];
        let acceptor = [0xBB; 32];
        let params = chat_request_session_id_params(&requester, Some("1234"), &acceptor, None);
        let mut expected = Vec::new();
        expected.extend_from_slice(&requester);
        expected.extend_from_slice(&acceptor);
        expected.extend_from_slice(b"/1234/");
        assert_eq!(params, expected);
    }

    #[test]
    fn session_topic_is_keyed_hash_of_context_and_params() {
        let secret = [0x44; 32];
        let requester = [0x55; 32];
        let acceptor = [0x66; 32];
        let params = chat_request_session_id_params(&requester, None, &acceptor, Some("9999"));
        let mut input = Vec::new();
        input.extend_from_slice(CHAT_REQUEST_CONTEXT);
        input.extend_from_slice(&params);
        let expected = blake2b_256_keyed(&secret, &input).unwrap();
        assert_eq!(
            chat_request_session_topic(&secret, &requester, None, &acceptor, Some("9999")).unwrap(),
            expected
        );
    }

    #[test]
    fn session_topic_rejects_non_32_byte_secret() {
        let requester = [0x55; 32];
        let acceptor = [0x66; 32];
        let err =
            chat_request_session_topic(&[0x44; 31], &requester, None, &acceptor, None).unwrap_err();
        assert!(err.to_string().contains("shared_secret must be 32 bytes"));
    }

    #[test]
    fn text_message_round_trips() {
        let encoded = encode_text_message("msg-1", 123, "hello").unwrap();
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-1".into(),
                timestamp: 123,
                content: V2ChatMessageContent::Text("hello".into())
            }
        );
    }

    #[test]
    fn chat_accepted_message_round_trips() {
        let encoded = encode_chat_accepted_message("msg-2", 456, "request-1").unwrap();
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-2".into(),
                timestamp: 456,
                content: V2ChatMessageContent::ChatAccepted {
                    request_id: "request-1".into()
                }
            }
        );
    }

    #[test]
    fn data_channel_offer_round_trips() {
        let encoded = encode_data_channel_offer_message(
            "msg-offer",
            457,
            b"offer-sdp",
            V2DataChannelPurpose::Video,
        )
        .unwrap();
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-offer".into(),
                timestamp: 457,
                content: V2ChatMessageContent::DataChannelOffer {
                    sdp: b"offer-sdp".to_vec(),
                    purpose: V2DataChannelPurpose::Video,
                }
            }
        );
    }

    #[test]
    fn data_channel_offer_lifts_to_transport_neutral_call_signal() {
        let decoded = decode_message(
            &encode_data_channel_offer_message(
                "msg-offer",
                457,
                b"offer-sdp",
                V2DataChannelPurpose::Video,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            decoded.as_call_signal(),
            Some(V2CallSignal::Offer {
                offer_id: "msg-offer".into(),
                sdp: b"offer-sdp".to_vec(),
                purpose: V2DataChannelPurpose::Video,
            })
        );
    }

    #[test]
    fn data_channel_answer_round_trips() {
        let encoded =
            encode_data_channel_answer_message("msg-answer", 458, "offer-1", b"answer-sdp")
                .unwrap();
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-answer".into(),
                timestamp: 458,
                content: V2ChatMessageContent::DataChannelAnswer {
                    offer_id: "offer-1".into(),
                    sdp: b"answer-sdp".to_vec(),
                }
            }
        );
    }

    #[test]
    fn data_channel_candidates_round_trips() {
        let encoded = encode_data_channel_candidates_message(
            "msg-candidates",
            459,
            "offer-2",
            b"candidate-batch",
        )
        .unwrap();
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-candidates".into(),
                timestamp: 459,
                content: V2ChatMessageContent::DataChannelCandidates {
                    offer_id: "offer-2".into(),
                    sdp: b"candidate-batch".to_vec(),
                }
            }
        );
    }

    #[test]
    fn data_channel_closed_round_trips() {
        let encoded = encode_data_channel_closed_message("msg-closed", 460, "offer-3").unwrap();
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-closed".into(),
                timestamp: 460,
                content: V2ChatMessageContent::DataChannelClosed {
                    offer_id: "offer-3".into(),
                }
            }
        );
    }

    #[test]
    fn transport_neutral_call_signal_encodes_to_v2_message() {
        let encoded = encode_call_signal_message(
            "msg-call",
            461,
            &V2CallSignal::Candidates {
                offer_id: "offer-4".into(),
                sdp: b"candidate-batch".to_vec(),
            },
        )
        .unwrap();

        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-call".into(),
                timestamp: 461,
                content: V2ChatMessageContent::DataChannelCandidates {
                    offer_id: "offer-4".into(),
                    sdp: b"candidate-batch".to_vec(),
                }
            }
        );
    }

    #[test]
    fn unsupported_content_preserves_header() {
        let mut encoded = Vec::new();
        encode_string(&mut encoded, "msg-3").unwrap();
        encoded.extend_from_slice(&789_u64.to_le_bytes());
        encoded.push(0);
        encoded.push(99); // 99 is not a known content index
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-3".into(),
                timestamp: 789,
                content: V2ChatMessageContent::UnsupportedContent { content_index: 99 }
            }
        );
    }

    #[test]
    fn unsupported_version_preserves_header() {
        let mut encoded = Vec::new();
        encode_string(&mut encoded, "msg-4").unwrap();
        encoded.extend_from_slice(&890_u64.to_le_bytes());
        encoded.push(1);
        encoded.extend_from_slice(b"future payload");
        assert_eq!(
            decode_message(&encoded).unwrap(),
            V2ChatMessage {
                message_id: "msg-4".into(),
                timestamp: 890,
                content: V2ChatMessageContent::UnsupportedVersion { version_index: 1 }
            }
        );
    }

    #[test]
    fn supported_content_rejects_trailing_bytes() {
        let mut encoded = encode_text_message("msg-5", 901, "hello").unwrap();
        encoded.push(0);
        let err = decode_message(&encoded).unwrap_err();
        assert!(err.to_string().contains("trailing bytes"));
    }

    #[test]
    fn invalid_data_channel_purpose_rejects_decode() {
        let mut encoded = Vec::new();
        encode_string(&mut encoded, "msg-invalid").unwrap();
        encoded.extend_from_slice(&902_u64.to_le_bytes());
        encoded.push(0);
        encoded.push(8);
        encode_bytes(&mut encoded, b"sdp").unwrap();
        encoded.push(9);

        let err = decode_message(&encoded).unwrap_err();
        assert!(err.to_string().contains("unsupported data channel purpose"));
    }

    #[test]
    fn test_token_message_roundtrip() {
        let encoded =
            encode_token_message("msg-1", 1000, &[0xAA, 0xBB], V2PushPlatform::Ios).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(decoded.message_id, "msg-1");
        assert_eq!(decoded.timestamp, 1000);
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::Token {
                token: vec![0xAA, 0xBB],
                platform: V2PushPlatform::Ios
            }
        );
    }

    #[test]
    fn token_message_decodes_v2_push_token_shape() {
        let mut encoded = Vec::new();
        encode_string(&mut encoded, "msg-token-push").unwrap();
        encoded.extend_from_slice(&1001_u64.to_le_bytes());
        encoded.push(0);
        encoded.push(1);
        encode_bytes(&mut encoded, &[0xAA, 0xBB, 0xCC]).unwrap();
        encoded.push(2);

        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::Token {
                token: vec![0xAA, 0xBB, 0xCC],
                platform: V2PushPlatform::IosVoip
            }
        );
    }

    #[test]
    fn test_send_legacy_message_roundtrip() {
        let block_hash = vec![0x11; 32];
        let extrinsic_hash = vec![0x22; 32];
        let encoded =
            encode_send_legacy_message("msg-2", 2000, "100", &block_hash, &extrinsic_hash).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::SendLegacy {
                amount: "100".into(),
                block_hash,
                extrinsic_hash
            }
        );
    }

    #[test]
    fn send_legacy_rejects_truncated_ios_payload_shape() {
        let mut encoded = Vec::new();
        encode_string(&mut encoded, "msg-send-legacy-real").unwrap();
        encoded.extend_from_slice(&2001_u64.to_le_bytes());
        encoded.push(0);
        encoded.push(2);
        encode_balance(&mut encoded, "100").unwrap();
        encoded.extend_from_slice(&[0x11; 31]);

        let err = decode_message(&encoded).unwrap_err();
        assert!(err.to_string().contains("truncated at block_hash"));
    }

    #[test]
    fn send_legacy_rejects_non_canonical_amount() {
        let encode = |amount: &[u8]| {
            let mut encoded = Vec::new();
            encode_string(&mut encoded, "msg-send-legacy").unwrap();
            encoded.extend_from_slice(&2001_u64.to_le_bytes());
            encoded.push(0);
            encoded.push(2);
            encoded.extend_from_slice(amount);
            encoded.extend_from_slice(&[0x11; 32]);
            encoded.extend_from_slice(&[0x22; 32]);
            encoded
        };

        assert!(decode_message(&encode(&[10 << 2])).is_ok());
        for amount in [
            &[(10 << 2) | 0b01, 0][..],
            &[(10 << 2) | 0b10, 0, 0, 0][..],
            &[0b11, 10, 0, 0, 0][..],
            &[(1 << 2) | 0b11, 0, 0, 0, 0x40, 0][..],
        ] {
            let err = decode_message(&encode(amount)).unwrap_err();
            assert!(
                err.to_string().contains("non-canonical"),
                "{amount:?}: {err}"
            );
        }
    }

    #[test]
    fn test_contact_added_message_roundtrip() {
        let encoded = encode_contact_added_message("msg-3", 3000).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(decoded.content, V2ChatMessageContent::ContactAdded);
    }

    #[test]
    fn test_reacted_message_roundtrip() {
        let encoded = encode_reacted_message("msg-4", 4000, "ref-1", "\u{1F44D}").unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::Reacted {
                message_id: "ref-1".into(),
                emoji: "\u{1F44D}".into()
            }
        );
    }

    #[test]
    fn test_reaction_removed_message_roundtrip() {
        let encoded =
            encode_reaction_removed_message("msg-5", 5000, "ref-2", "\u{2764}\u{FE0F}").unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::ReactionRemoved {
                message_id: "ref-2".into(),
                emoji: "\u{2764}\u{FE0F}".into()
            }
        );
    }

    #[test]
    fn test_reply_message_roundtrip() {
        let encoded = encode_reply_message("msg-6", 6000, "ref-3", Some("reply text")).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::Reply {
                message_id: "ref-3".into(),
                text: Some("reply text".into()),
                attachments: None,
            }
        );
    }

    #[test]
    fn test_reply_message_without_text_roundtrip() {
        let encoded = encode_reply_message("msg-6b", 6001, "ref-3b", None).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::Reply {
                message_id: "ref-3b".into(),
                text: None,
                attachments: None,
            }
        );
    }

    #[test]
    fn test_edited_message_roundtrip() {
        let encoded = encode_edited_message("msg-7", 7000, "ref-4", Some("new content")).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::Edited {
                message_id: "ref-4".into(),
                new_text: Some("new content".into()),
                attachments: None,
            }
        );
    }

    #[test]
    fn test_left_chat_message_roundtrip() {
        let encoded = encode_left_chat_message("msg-8", 8000).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(decoded.content, V2ChatMessageContent::LeftChat);
    }

    #[test]
    fn test_rich_text_message_roundtrip() {
        let encoded = encode_rich_text_message("msg-9", 9000, Some("hello rich"), None).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::RichText {
                text: Some("hello rich".into()),
                attachments: None,
            }
        );
    }

    #[test]
    fn egui_rich_text_matches_native_chat_v2_vector() {
        let encoded = encode_rich_text_message(
            "egui-message",
            1_700_000_000_123,
            Some("hello from egui"),
            None,
        )
        .unwrap();
        assert_eq!(
            hex::encode(&encoded),
            "30656775692d6d6573736167657b68e5cf8b010000000f013c68656c6c6f2066726f6d206567756900"
        );

        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(decoded.message_id, "egui-message");
        assert_eq!(decoded.timestamp, 1_700_000_000_123);
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::RichText {
                text: Some("hello from egui".into()),
                attachments: None,
            }
        );
    }

    #[test]
    fn test_rich_text_message_without_text_roundtrip() {
        let encoded = encode_rich_text_message("msg-9b", 9001, None, None).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::RichText {
                text: None,
                attachments: None
            }
        );
    }

    // Native brevity-chat wire.rs field order/indices and fixed-u32 numeric
    // fixture, with real 32-byte HOP entry hashes and FileTicket keys.
    fn native_attachment_fixture(content_index: u8, meta_index: u8) -> Vec<u8> {
        let mut bytes = vec![4, b'm', 1, 0, 0, 0, 0, 0, 0, 0, 0, content_index];
        if content_index == 7 || content_index == 12 {
            bytes.extend_from_slice(&[4, b'r']);
        }
        bytes.extend_from_slice(&[0, 1, 4, 0, 0x80]); // None text, Some(one file), hash length
        bytes.extend_from_slice(&[0xa1; 32]);
        bytes.push(0x80); // ticket length
        bytes.extend_from_slice(&[0xb1; 32]);
        bytes.extend_from_slice(&[0, 0x1c]); // WssUrl, length 7
        bytes.extend_from_slice(b"wss://n");
        bytes.push(meta_index);
        match meta_index {
            0 => {
                bytes.push(0x28);
                bytes.extend_from_slice(b"text/plain");
                bytes.extend_from_slice(&[0xff; 4]); // file_size = u32::MAX
            }
            1 => {
                bytes.push(0x28);
                bytes.extend_from_slice(b"image/jpeg");
                bytes.extend_from_slice(&[0x40, 0xe2, 1, 0]); // file_size = 123456
                bytes.extend_from_slice(&[0x20, 3, 0, 0]); // width = 800
                bytes.extend_from_slice(&[0x58, 2, 0, 0]); // height = 600
                bytes.extend_from_slice(&[1, 0x10, b'L', b'K', b'O', b'2']);
            }
            2 => {
                bytes.push(0x24);
                bytes.extend_from_slice(b"video/mp4");
                bytes.extend_from_slice(&[7, 0, 0, 0]); // file_size = 7
                bytes.extend_from_slice(&[90, 0, 0, 0, 0]); // duration = 90, no thumbnail
            }
            _ => unreachable!(),
        }
        bytes
    }

    fn native_attachment(meta: V2FileMeta) -> V2FileVariant {
        V2FileVariant::P2pMixnet(V2P2pMixnetFile {
            identifier: vec![0xa1; 32],
            claim_ticket: vec![0xb1; 32],
            node: V2NodeEndpoint::WssUrl("wss://n".into()),
            meta,
        })
    }

    #[test]
    fn native_ordinary_reply_and_edit_attachment_fixtures() {
        let cases = [
            (
                7,
                0,
                V2FileMeta::General(V2GeneralFileMeta {
                    mime_type: "text/plain".into(),
                    file_size: u32::MAX,
                }),
            ),
            (
                15,
                1,
                V2FileMeta::Image(V2ImageFileMeta {
                    general: V2GeneralFileMeta {
                        mime_type: "image/jpeg".into(),
                        file_size: 123_456,
                    },
                    width: 800,
                    height: 600,
                    thumbnail: Some(b"LKO2".to_vec()),
                }),
            ),
            (
                12,
                2,
                V2FileMeta::Video(V2VideoFileMeta {
                    general: V2GeneralFileMeta {
                        mime_type: "video/mp4".into(),
                        file_size: 7,
                    },
                    duration: 90,
                    thumbnail: None,
                }),
            ),
        ];
        for (content_index, meta_index, meta) in cases {
            let attachments = vec![native_attachment(meta)];
            let expected = match content_index {
                7 => V2ChatMessageContent::Reply {
                    message_id: "r".into(),
                    text: None,
                    attachments: Some(attachments.clone()),
                },
                12 => V2ChatMessageContent::Edited {
                    message_id: "r".into(),
                    new_text: None,
                    attachments: Some(attachments.clone()),
                },
                _ => V2ChatMessageContent::RichText {
                    text: None,
                    attachments: Some(attachments.clone()),
                },
            };
            assert_eq!(
                decode_message(&native_attachment_fixture(content_index, meta_index)).unwrap(),
                V2ChatMessage {
                    message_id: "m".into(),
                    timestamp: 1,
                    content: expected
                },
            );
            assert_eq!(
                encode_rich_text_message("m", 1, None, Some(&attachments)).unwrap(),
                native_attachment_fixture(15, meta_index),
            );
        }
    }

    #[test]
    fn rich_attachment_options_preserve_empty_and_multiple_files() {
        let file = native_attachment(V2FileMeta::General(V2GeneralFileMeta {
            mime_type: "text/plain".into(),
            file_size: 0,
        }));
        for attachments in [Some(vec![]), Some(vec![file.clone(), file]), None] {
            let encoded =
                encode_rich_text_message("m", 1, Some("caption"), attachments.as_deref()).unwrap();
            assert_eq!(
                decode_message(&encoded).unwrap().content,
                V2ChatMessageContent::RichText {
                    text: Some("caption".into()),
                    attachments
                }
            );
        }
    }

    #[test]
    fn attachment_decoder_rejects_malformed_boundaries() {
        let valid = native_attachment_fixture(15, 1);
        // Each offset is a different SCALE discriminant in the native image fixture.
        for (offset, value) in [(12, 2), (13, 2), (15, 1), (82, 1), (91, 3), (115, 2)] {
            let mut malformed = valid.clone();
            malformed[offset] = value;
            assert!(
                decode_message(&malformed).is_err(),
                "discriminant at {offset}"
            );
        }
        // The hash and ticket are Vec<u8> on wire but must be exact native keys.
        for offset in [16, 49] {
            let mut short = valid.clone();
            short[offset] = 31 << 2;
            short.remove(offset + 1);
            assert!(decode_message(&short).is_err());
            let mut long = valid.clone();
            long[offset] = 33 << 2;
            long.insert(offset + 1, 0);
            assert!(decode_message(&long).is_err());
        }
        // Counts and variable lengths must be checked before allocation.
        for offset in [14, 83, 92, 116] {
            let mut oversized = valid[..offset].to_vec();
            oversized.extend_from_slice(&[3, 255, 255, 255, 255]); // compact u32::MAX
            oversized.extend_from_slice(&valid[offset + 1..]);
            assert!(decode_message(&oversized).is_err(), "length at {offset}");
        }
        let mut noncanonical = valid[..14].to_vec();
        noncanonical.extend_from_slice(&[5, 0]); // noncanonical compact count 1
        noncanonical.extend_from_slice(&valid[15..]);
        assert!(decode_message(&noncanonical).is_err());
        for end in 0..valid.len() {
            assert!(decode_message(&valid[..end]).is_err(), "truncated at {end}");
        }
        let mut trailing = valid;
        trailing.push(0);
        assert!(decode_message(&trailing).is_err());
    }

    #[test]
    fn attachment_encoder_rejects_wrong_key_widths_and_unsafe_nodes() {
        let V2FileVariant::P2pMixnet(file) =
            native_attachment(V2FileMeta::General(V2GeneralFileMeta {
                mime_type: "text/plain".into(),
                file_size: 0,
            }));
        let mut short_hash = file.clone();
        short_hash.identifier.pop();
        let mut long_ticket = file.clone();
        long_ticket.claim_ticket.push(0);
        for invalid in [short_hash, long_ticket] {
            assert!(
                encode_rich_text_message("m", 1, None, Some(&[V2FileVariant::P2pMixnet(invalid),]))
                    .is_err()
            );
        }
        for url in [
            "ws://n",
            "wss://",
            "wss://user@n",
            "wss://n#fragment",
            "wss://n:65536",
            "wss://[bad]",
            "wss://n\n/path",
        ] {
            let mut invalid = file.clone();
            invalid.node = V2NodeEndpoint::WssUrl(url.into());
            assert!(
                encode_rich_text_message("m", 1, None, Some(&[V2FileVariant::P2pMixnet(invalid),]))
                    .is_err()
            );
        }
        let mut invalid_wire = native_attachment_fixture(15, 0);
        invalid_wire[84] = b'x'; // xss://n is not a secure WebSocket endpoint.
        assert!(decode_message(&invalid_wire).is_err());
    }

    #[test]
    fn invitation_decoders_do_not_drop_attachments() {
        let rich = native_attachment_fixture(15, 1);
        let mut legacy = vec![4, b'm', 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        legacy.extend_from_slice(&rich[12..]);
        assert!(decode_chat_request_message(&legacy).is_err());
        let mut current = vec![4, b'm', 1, 0, 0, 0, 0, 0, 0, 0, 1];
        current.extend_from_slice(&[1; 96]); // identity, proof, device encryption key
        current.extend_from_slice(&[0, 1]); // no push token, Some welcome
        current.extend_from_slice(&rich[12..]);
        assert!(decode_chat_request_message_v2(&current).is_err());
    }

    #[test]
    fn test_coinage_send_message_roundtrip() {
        let coin_keys = vec![vec![0xAAu8; 32], vec![0xBBu8; 32]];
        let encoded = encode_coinage_send_message("msg-10", 10000, "1000", &coin_keys).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::CoinageSend {
                total_value: "1000".into(),
                coin_keys: coin_keys.clone()
            }
        );
    }
    #[test]
    fn profile_reference_wire_roundtrips_disclosure_and_withdrawal() {
        let disclosed = encode_profile_reference_message(
            "profile",
            5,
            "seity.dot",
            Some("seity-contacts:v1:00"),
        )
        .unwrap();
        let decoded = decode_message(&disclosed).unwrap();
        assert_eq!(decoded.message_id, "profile");
        assert_eq!(
            decoded.content,
            V2ChatMessageContent::ProfileReference {
                discloser_product_id: "seity.dot".into(),
                reference: Some("seity-contacts:v1:00".into()),
            }
        );
        let withdrawn = encode_profile_reference_message("profile", 6, "seity.dot", None).unwrap();
        assert_eq!(
            decode_message(&withdrawn).unwrap().content,
            V2ChatMessageContent::ProfileReference {
                discloser_product_id: "seity.dot".into(),
                reference: None,
            }
        );
        // A malformed option byte is refused, not guessed at.
        let mut bad = withdrawn.clone();
        *bad.last_mut().unwrap() = 7;
        assert!(decode_message(&bad).is_err());
    }

    #[test]
    fn current_multi_device_wire_roundtrips() {
        let added = encode_device_added_message("add", 1, &[1; 32], &[2; 32]).unwrap();
        assert!(matches!(
            decode_message(&added).unwrap().content,
            V2ChatMessageContent::DeviceAdded { .. }
        ));
        let removed = encode_device_removed_message("remove", 2, &[1; 32]).unwrap();
        assert!(matches!(
            decode_message(&removed).unwrap().content,
            V2ChatMessageContent::DeviceRemoved { .. }
        ));
        let compacted = encode_compacted_messages_message(
            "compact",
            3,
            &[3; 4],
            &[4; 5],
            &V2NodeEndpoint::WssUrl("wss://chat.example".into()),
        )
        .unwrap();
        assert!(matches!(
            decode_message(&compacted).unwrap().content,
            V2ChatMessageContent::CompactedMessages { .. }
        ));
        let accepted = encode_multi_chat_accepted_message(
            "accept",
            4,
            "request",
            &V2PeerDevice {
                statement_account_id: [5; 32],
                encryption_public_key: [6; 32],
            },
        )
        .unwrap();
        assert_eq!(accepted[16], 20, "Android DeviceChatAccepted wire index");
        assert!(matches!(
            decode_message(&accepted).unwrap().content,
            V2ChatMessageContent::MultiChatAccepted { .. }
        ));
    }

    #[test]
    fn request_content_v2_seals_and_opens_with_fixed_nonce() {
        let request = V2ChatRequestV2 {
            message: V2ChatRequestMessageV2 {
                message_id: "request-v2".into(),
                timestamp: 42,
                content: V2ChatRequestContentV2 {
                    identity_proof: V2ChatRequestIdentityProof {
                        identity_account_id: [7; 32],
                        proof: [8; 32],
                    },
                    device_enc_pub_key: [9; 32],
                    push_token: None,
                    welcome_text: Some("hello".into()),
                },
            },
            proof: V2ChatRequestProof {
                signature: vec![10; 64],
                signer: vec![11; 32],
            },
        };
        let recipient_private = [12; 32];
        let wrapper = seal_chat_request_v2_with_nonce(
            &[13; 32],
            &x25519_public_key(&recipient_private),
            &request,
            [14; 12],
        )
        .unwrap();
        assert_eq!(
            open_chat_request_v2(&recipient_private, &wrapper).unwrap(),
            request
        );
    }
    #[test]
    fn context_bound_invite_authenticates_product_accounts_and_route() {
        let sender_account_id = [7; 32];
        let recipient_account_id = [12; 32];
        let channel_id = [15; 32];
        let request = V2ChatRequestV2 {
            message: V2ChatRequestMessageV2 {
                message_id: "context-bound".into(),
                timestamp: 42,
                content: V2ChatRequestContentV2 {
                    identity_proof: V2ChatRequestIdentityProof {
                        identity_account_id: sender_account_id,
                        proof: [8; 32],
                    },
                    device_enc_pub_key: [9; 32],
                    push_token: None,
                    welcome_text: Some("secure hello".into()),
                },
            },
            proof: V2ChatRequestProof {
                signature: vec![10; 64],
                signer: vec![11; 32],
            },
        };
        let recipient_private = [12; 32];
        let wrapper = seal_context_bound_chat_request_v2_with_nonce(
            &[13; 32],
            &x25519_public_key(&recipient_private),
            "egui-chat.paseo",
            &sender_account_id,
            &recipient_account_id,
            &channel_id,
            &request,
            [14; 12],
        )
        .unwrap();
        assert_eq!(
            open_context_bound_chat_request_v2(
                &recipient_private,
                "egui-chat.paseo",
                &recipient_account_id,
                &channel_id,
                &wrapper,
            )
            .unwrap(),
            request
        );
        assert!(
            open_context_bound_chat_request_v2(
                &recipient_private,
                "egui-chat.westend",
                &recipient_account_id,
                &channel_id,
                &wrapper,
            )
            .is_err()
        );
        assert!(
            open_context_bound_chat_request_v2(
                &recipient_private,
                "egui-chat.paseo",
                &recipient_account_id,
                &[16; 32],
                &wrapper,
            )
            .is_err()
        );
        assert!(open_chat_request_v2(&recipient_private, &wrapper).is_err());
        assert!(decode_context_bound_chat_request_v2(&[99; 32], &channel_id, &wrapper).is_err());
        assert!(
            decode_context_bound_chat_request_v2(
                &recipient_account_id,
                &channel_id,
                &wrapper[..163]
            )
            .is_err()
        );
        let mut unsupported_version = wrapper.clone();
        unsupported_version[5] = 4;
        assert!(is_context_bound_chat_request_v2(&unsupported_version));
        assert!(
            decode_context_bound_chat_request_v2(
                &recipient_account_id,
                &channel_id,
                &unsupported_version,
            )
            .is_err()
        );
    }

    #[test]
    fn bare_message_exchange_has_no_statement_data_index() {
        let request =
            encode_message_exchange_request_plaintext("inner", &[vec![0xaa, 0xbb]]).unwrap();
        assert_eq!(
            request,
            [vec![0x14], b"inner".to_vec(), vec![0x04, 0x08, 0xaa, 0xbb],].concat()
        );
        assert_eq!(
            decode_message_exchange_request_plaintext(&request).unwrap(),
            V2MessageExchangeRequest {
                request_id: "inner".into(),
                messages: vec![vec![0xaa, 0xbb]],
            }
        );
        let response = encode_message_exchange_response_plaintext("inner", 0).unwrap();
        assert_eq!(response, [vec![0x14], b"inner".to_vec(), vec![0]].concat());
        assert_eq!(
            decode_message_exchange_response_plaintext(&response).unwrap(),
            V2MessageExchangeResponse {
                request_id: "inner".into(),
                response_code: 0,
            }
        );
    }

    #[test]
    fn oversized_collection_counts_are_rejected_before_allocation() {
        assert!(decode_transport_plaintext(&[2, 0, 3, 0xff, 0xff, 0xff, 0xff]).is_err());
        assert!(
            decode_message_exchange_request_plaintext(&[0, 3, 0xff, 0xff, 0xff, 0xff]).is_err()
        );
    }
}
