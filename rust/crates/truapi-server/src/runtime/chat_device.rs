//! Host-private native Chat multi-device encryption and content classification.
//!
//! The caller must authenticate the statement sender and its account/key binding,
//! authorize the recipient roster, and open the identity route before calling this
//! module. Native multi-device envelopes bind recipient keys, not account names or
//! the outer transport kind; those are authenticated by that enclosing route.

mod rich;
pub(crate) use rich::{MAX_ATTACHMENTS, RichContent, validate_metadata};

use parity_scale_codec::{Compact, Decode, Encode};
use truapi::latest::HostNativeChatRichMessageKind as RichKind;
use useragent_chat_v2::{self as chat, V2ChatMessageContent, V2StatementTransportData};
use zeroize::Zeroizing;

/// Maximum authenticated devices in a native Chat recipient roster.
pub(crate) const MAX_CHAT_PEERS: usize = 16;
/// Maximum encoded native Chat envelope accepted by the Host.
pub(crate) const MAX_CHAT_ENVELOPE_BYTES: usize = 256 * 1024;
const MAX_MESSAGES: usize = 256;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 8 * 1024;
const MAX_EMOJI_BYTES: usize = 64;
const AEAD_OVERHEAD: usize = 12 + 16;
const WRAPPED_KEY_BYTES: usize = 32 + AEAD_OVERHEAD;

/// A device whose account/key association the Host has already authenticated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PeerDevice {
    /// Statement signing account, not the identity account.
    pub(crate) account_id: [u8; 32],
    /// Canonical X25519 device encryption key.
    pub(crate) public_key: [u8; 32],
}

/// Payload-free failures safe to propagate across the Host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub(crate) enum ChatDeviceError {
    /// An input exceeds the bounded Chat protocol surface.
    #[display("Chat device input exceeds its size limit")]
    LimitExceeded,
    /// SCALE framing, supported content, or fixed-width fields are malformed.
    #[display("Invalid Chat device encoding")]
    InvalidEncoding,
    /// A key is noncanonical, noncontributory, or aliases this device's account.
    #[display("Invalid Chat device key")]
    InvalidPeerKey,
    /// The roster is empty or repeats an account or encryption key.
    #[display("Invalid Chat device roster")]
    InvalidRoster,
    /// The envelope has no recipient entry for this device.
    #[display("Chat envelope is addressed to another device")]
    WrongRecipient,
    /// The sender/key binding or encrypted payload failed authentication.
    #[display("Chat device authentication failed")]
    AuthenticationFailed,
    /// The Host cannot safely interpret this content variant.
    #[display("Unsupported Chat device content")]
    UnsupportedContent,
    /// Guest data attempts a Host-owned payment or lifecycle operation.
    #[display("Chat content requires Host authority")]
    ForbiddenContent,
    /// Secure platform randomness is unavailable.
    #[display("Chat device randomness unavailable")]
    RandomnessUnavailable,
}

/// A non-exportable Chat device secret. It is never a Coinage derivation root.
pub(crate) struct HostChatDevice {
    account_id: [u8; 32],
    secret: Zeroizing<[u8; 32]>,
    public_key: [u8; 32],
}

/// A decoded exchange; secret-bearing messages deliberately have no Debug/Clone.
pub(crate) enum OpenedDeviceExchange {
    /// A strictly decoded request retaining wire order and acknowledgement ID.
    Request {
        /// Correlation ID to acknowledge only after durable Host processing.
        request_id: String,
        /// Ordered content; controls and payments must never be forwarded raw.
        messages: Vec<OpenedDeviceMessage>,
    },
    /// Native response codes are raw bytes: zero succeeds, nonzero fails.
    Response {
        /// Correlation ID of the acknowledged request.
        request_id: String,
        /// Native success/failure code, without reinterpretation.
        response_code: u8,
    },
}

/// Exactly one classified message in its original position in the request.
pub(crate) enum OpenedDeviceMessage {
    /// Fully decoded, supported ordinary content in its original native encoding.
    Ordinary(Vec<u8>),
    /// Host-owned device/session lifecycle change, not an authorized mutation.
    DeviceControl(DeviceControl),
    /// Host-private spendable material; never a guest-visible payment event.
    Payment(PaymentMemo),
    /// Private HOP reference; expand and classify before either kind of ACK.
    CompactedHistory(CompactedHistory),
    /// Rich content with private file credentials separated from public metadata.
    RichContent(RichContent),
    /// Native notification metadata, never ordinary guest content. This Host has
    /// no mobile push provider; retain only ordering and replay evidence.
    PushToken { timestamp: u64, digest: [u8; 32] },
}

/// Lifecycle metadata to validate against durable Host roster and replay state.
pub(crate) struct DeviceControl {
    /// Native message identifier.
    pub(crate) message_id: String,
    /// Native message timestamp; ordering/replay authorization belongs to Host.
    pub(crate) timestamp: u64,
    /// Typed lifecycle operation without an arbitrary encoded-message escape.
    pub(crate) content: DeviceLifecycle,
}

/// Native lifecycle variants requiring Host authorization before application.
pub(crate) enum DeviceLifecycle {
    /// Add an independently authenticated device to the sender identity.
    Added(PeerDevice),
    /// Revoke a device's statement account.
    Removed([u8; 32]),
    /// Accept a native single-device request.
    Accepted {
        /// Native request correlation ID.
        request_id: String,
    },
    /// Accept a native multi-device request and advertise its device binding.
    MultiAccepted {
        /// Native request correlation ID.
        request_id: String,
        /// Advertised binding to authenticate before adding it to a roster.
        device: PeerDevice,
    },
    /// Native contact-added notification.
    ContactAdded,
    /// Native session departure notification.
    LeftChat,
}

/// Native content-index-16 memo; every owned secret buffer zeroizes on drop.
pub(crate) struct PaymentMemo {
    /// Native message identifier for durable receive deduplication.
    pub(crate) message_id: String,
    /// Native message timestamp.
    pub(crate) timestamp: u64,
    /// Declared amount, which the Coinage engine must verify against the chain.
    pub(crate) total_value: u128,
    /// Native 64-byte coin secrets, retained exclusively by the Host.
    pub(crate) coin_keys: Zeroizing<Vec<Vec<u8>>>,
}

/// An authenticated reference to encrypted native history, never guest content.
pub(crate) struct CompactedHistory {
    pub(crate) message_id: String,
    pub(crate) timestamp: u64,
    pub(crate) identifier: [u8; 32],
    pub(crate) ticket: Zeroizing<[u8; 32]>,
    pub(crate) endpoint: String,
}

impl HostChatDevice {
    /// Take custody of an already Host-private device key and statement account.
    pub(crate) fn from_secret(account: [u8; 32], secret: [u8; 32]) -> Self {
        let secret = Zeroizing::new(secret);
        let public_key = chat::x25519_public_key(&secret);
        Self {
            account_id: account,
            secret,
            public_key,
        }
    }

    /// Public device key safe to advertise in an authenticated device binding.
    pub(crate) fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    /// Derive the native device-to-identity outer route without exporting a key.
    pub(crate) fn identity_shared_secret(
        &self,
        peer_identity_key: &[u8; 32],
    ) -> Result<Zeroizing<[u8; 32]>, ChatDeviceError> {
        validate_public_key(peer_identity_key)?;
        chat::x25519_shared_secret(&self.secret, peer_identity_key)
            .map(Zeroizing::new)
            .map_err(|_| ChatDeviceError::InvalidPeerKey)
    }

    /// Open canonical StatementData after the authenticated identity route opens.
    ///
    /// `sender` must come from the verified statement and Host-private roster,
    /// never an unauthenticated guest account/key pair. Nothing returns until the
    /// entire inner request has classified successfully; there is no raw fallback.
    pub(crate) fn open_multi_device(
        &self,
        sender: &PeerDevice,
        envelope_bytes: &[u8],
    ) -> Result<OpenedDeviceExchange, ChatDeviceError> {
        preflight_envelope(envelope_bytes)?;
        let wrapping_key = self.pairwise_key(sender)?;
        let (is_request, encrypted, devices) =
            match chat::decode_transport_plaintext(envelope_bytes)
                .map_err(|_| ChatDeviceError::InvalidEncoding)?
            {
                V2StatementTransportData::MultiRequest(request) => {
                    (true, request.encrypted_request, request.devices_info)
                }
                V2StatementTransportData::MultiResponse(response) => {
                    (false, response.encrypted_response, response.devices_info)
                }
                _ => return Err(ChatDeviceError::UnsupportedContent),
            };
        for (index, device) in devices.iter().enumerate() {
            if devices[..index]
                .iter()
                .any(|previous| previous.statement_account_id == device.statement_account_id)
            {
                return Err(ChatDeviceError::InvalidRoster);
            }
        }
        let own_entry = devices
            .iter()
            .find(|device| device.statement_account_id == self.account_id)
            .ok_or(ChatDeviceError::WrongRecipient)?;
        // The codec's unwrap helper leaves its temporary Vec unguarded. Using the
        // same codec AEAD primitive directly lets us guard both length-error and
        // success paths without copying the unwrapped one-shot key.
        let one_shot_bytes = Zeroizing::new(
            chat::decrypt_multi_device_payload(&wrapping_key, &own_entry.encrypted_key)
                .map_err(|_| ChatDeviceError::AuthenticationFailed)?,
        );
        let one_shot_key: &[u8; 32] = one_shot_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ChatDeviceError::InvalidEncoding)?;
        let plaintext = Zeroizing::new(
            chat::decrypt_multi_device_payload(one_shot_key, &encrypted)
                .map_err(|_| ChatDeviceError::AuthenticationFailed)?,
        );
        if is_request {
            classify_request(&plaintext)
        } else {
            let response = decode_response(&plaintext)?;
            Ok(OpenedDeviceExchange::Response {
                request_id: response.request_id,
                response_code: response.response_code,
            })
        }
    }

    /// Seal a Host-authorized request/response for an authenticated recipient roster.
    ///
    /// Input is the codec's **tagged** `encode_transport_request_plaintext` or
    /// `encode_transport_response_plaintext` output. Only the bare inner payload
    /// is encrypted, preserving native MultiRequest/MultiResponse wire semantics.
    /// This primitive is not a guest generic Seal API: the caller must authorize
    /// every message, especially lifecycle controls and outgoing payment memos.
    pub(crate) fn seal_multi_device(
        &self,
        peers: &[PeerDevice],
        inner_plaintext: &[u8],
    ) -> Result<Vec<u8>, ChatDeviceError> {
        check_size(inner_plaintext.len())?;
        if peers.is_empty() || peers.len() > MAX_CHAT_PEERS {
            return Err(ChatDeviceError::InvalidRoster);
        }
        for (index, peer) in peers.iter().enumerate() {
            if peers[..index].iter().any(|previous| {
                previous.account_id == peer.account_id || previous.public_key == peer.public_key
            }) {
                return Err(ChatDeviceError::InvalidRoster);
            }
        }
        let (&kind, body) = inner_plaintext
            .split_first()
            .ok_or(ChatDeviceError::InvalidEncoding)?;
        match kind {
            0 => preflight_request(body)?,
            1 => {
                decode_response(body)?;
            }
            _ => return Err(ChatDeviceError::UnsupportedContent),
        }
        let encrypted_len = body.len() + AEAD_OVERHEAD;
        let envelope_len = 1
            + Compact(encrypted_len as u32).encoded_size()
            + encrypted_len
            + Compact(peers.len() as u32).encoded_size()
            + peers.len()
                * (32 + Compact(WRAPPED_KEY_BYTES as u32).encoded_size() + WRAPPED_KEY_BYTES);
        check_size(envelope_len)?;
        let wrapping_keys = peers
            .iter()
            .map(|peer| self.pairwise_key(peer))
            .collect::<Result<Vec<_>, _>>()?;
        let mut one_shot_key = Zeroizing::new([0; 32]);
        getrandom::getrandom(one_shot_key.as_mut())
            .map_err(|_| ChatDeviceError::RandomnessUnavailable)?;
        let encrypted =
            chat::encrypt_multi_device_payload_with_nonce(&one_shot_key, body, random_nonce()?)
                .map_err(|_| ChatDeviceError::AuthenticationFailed)?;
        let mut devices_info = Vec::with_capacity(peers.len());
        for (peer, wrapping_key) in peers.iter().zip(&wrapping_keys) {
            let encrypted_key = chat::encrypt_multi_device_payload_with_nonce(
                wrapping_key,
                one_shot_key.as_ref(),
                random_nonce()?,
            )
            .map_err(|_| ChatDeviceError::AuthenticationFailed)?;
            devices_info.push(chat::V2RequestDeviceInfo {
                statement_account_id: peer.account_id,
                encrypted_key,
            });
        }
        match kind {
            0 => chat::encode_transport_multi_request_plaintext(&chat::V2MultiDeviceRequest {
                encrypted_request: encrypted,
                devices_info,
            }),
            _ => chat::encode_transport_multi_response_plaintext(&chat::V2MultiDeviceResponse {
                encrypted_response: encrypted,
                devices_info,
            }),
        }
        .map_err(|_| ChatDeviceError::InvalidEncoding)
    }

    fn pairwise_key(&self, peer: &PeerDevice) -> Result<Zeroizing<[u8; 32]>, ChatDeviceError> {
        let own_key = self.public_key();
        if (peer.account_id == self.account_id) != (peer.public_key == own_key) {
            return Err(ChatDeviceError::InvalidPeerKey);
        }
        check_canonical_key(&peer.public_key)?;
        let shared = Zeroizing::new(
            chat::x25519_shared_secret(&self.secret, &peer.public_key)
                .map_err(|_| ChatDeviceError::InvalidPeerKey)?,
        );
        chat::hkdf_sha256_32(shared.as_ref())
            .map(Zeroizing::new)
            .map_err(|_| ChatDeviceError::InvalidPeerKey)
    }
}

/// Reject guest payment/control injection, unsupported variants, and attachments.
///
/// The allowlist is intentionally semantic, not a top-level byte denylist: the
/// codec fully parses reply/edit rich text; this boundary rejects private file references.
pub(crate) fn validate_guest_messages(messages: &[Vec<u8>]) -> Result<(), ChatDeviceError> {
    if messages.len() > MAX_MESSAGES {
        return Err(ChatDeviceError::LimitExceeded);
    }
    let mut total = 0usize;
    for bytes in messages {
        total = total
            .checked_add(bytes.len())
            .ok_or(ChatDeviceError::LimitExceeded)?;
        check_size(total)?;
        let (_, content_index, _) = message_header(bytes)?;
        match content_index {
            0 | 4 | 5 | 7 | 12 | 15 => {}
            3 | 13 | 14 | 16 | 17 | 18 | 20 => return Err(ChatDeviceError::ForbiddenContent),
            _ => return Err(ChatDeviceError::UnsupportedContent),
        }
        let message = chat::decode_message(bytes).map_err(|_| ChatDeviceError::InvalidEncoding)?;
        validate_ordinary(&message.content)?;
    }
    Ok(())
}

/// Classify a Host-decrypted identity exchange without exporting its plaintext.
/// The caller permits only the control messages valid for its handshake state.
pub(crate) fn open_identity_exchange(
    plaintext: &[u8],
) -> Result<OpenedDeviceExchange, ChatDeviceError> {
    check_size(plaintext.len())?;
    match plaintext.split_first() {
        Some((0, body)) => classify_request(body),
        Some((1, body)) => {
            let response = decode_response(body)?;
            Ok(OpenedDeviceExchange::Response {
                request_id: response.request_id,
                response_code: response.response_code,
            })
        }
        _ => Err(ChatDeviceError::InvalidEncoding),
    }
}

fn classify_request(plaintext: &[u8]) -> Result<OpenedDeviceExchange, ChatDeviceError> {
    // Preflight without copying: the codec's allocating request decoder can drop
    // earlier secret-bearing messages unzeroized when a later frame is truncated.
    preflight_request(plaintext)?;
    let request = chat::decode_message_exchange_request_plaintext(plaintext)
        .map_err(|_| ChatDeviceError::InvalidEncoding)?;
    let mut raw_messages = Zeroizing::new(request.messages);
    let messages = raw_messages
        .iter_mut()
        .map(classify_message)
        .collect::<Result<_, _>>()?;
    Ok(OpenedDeviceExchange::Request {
        request_id: request.request_id,
        messages,
    })
}

/// Classify one owned frame, including a frame recovered from private HOP history.
/// The caller zeroizes the source buffer on every success or failure path.
pub(crate) fn classify_message(
    bytes: &mut Vec<u8>,
) -> Result<OpenedDeviceMessage, ChatDeviceError> {
    let (mut content, index, timestamp) = message_header(bytes)?;
    match index {
        1 => {
            // Validate the native Token frame without allocating or retaining its
            // notification credential. It must not poison an acceptance batch.
            data(&mut content)?;
            if !matches!(take(&mut content, 1)?[0], 0..=2) {
                return Err(ChatDeviceError::InvalidEncoding);
            }
            finish(content)?;
            return Ok(OpenedDeviceMessage::PushToken {
                timestamp,
                digest: sp_crypto_hashing::blake2_256(bytes),
            });
        }
        16 => preflight_payment(&mut content)?,
        19 => preflight_compaction(&mut content)?,
        _ => {}
    }
    let message = chat::decode_message(bytes).map_err(|_| ChatDeviceError::InvalidEncoding)?;
    let control = match message.content {
        V2ChatMessageContent::RichText {
            text,
            attachments: Some(files),
        } if !files.is_empty() => {
            return rich::classify(
                message.message_id,
                message.timestamp,
                RichKind::Message,
                text,
                files,
                bytes,
            );
        }
        V2ChatMessageContent::Reply {
            message_id,
            text,
            attachments: Some(files),
        } if !files.is_empty() => {
            return rich::classify(
                message.message_id,
                message.timestamp,
                RichKind::Reply { message_id },
                text,
                files,
                bytes,
            );
        }
        V2ChatMessageContent::Edited {
            message_id,
            new_text,
            attachments: Some(files),
        } if !files.is_empty() => {
            return rich::classify(
                message.message_id,
                message.timestamp,
                RichKind::Edited { message_id },
                new_text,
                files,
                bytes,
            );
        }
        V2ChatMessageContent::CoinageSend {
            total_value,
            coin_keys,
        } => {
            let coin_keys = Zeroizing::new(coin_keys);
            let total_value = total_value
                .parse()
                .map_err(|_| ChatDeviceError::InvalidEncoding)?;
            return Ok(OpenedDeviceMessage::Payment(PaymentMemo {
                message_id: message.message_id,
                timestamp: message.timestamp,
                total_value,
                coin_keys,
            }));
        }
        V2ChatMessageContent::CompactedMessages {
            claim_identifier,
            claim_ticket,
            node,
        } => {
            let ticket = Zeroizing::new(claim_ticket);
            let chat::V2NodeEndpoint::WssUrl(endpoint) = node;
            return Ok(OpenedDeviceMessage::CompactedHistory(CompactedHistory {
                message_id: message.message_id,
                timestamp: message.timestamp,
                identifier: claim_identifier
                    .as_slice()
                    .try_into()
                    .map_err(|_| ChatDeviceError::InvalidEncoding)?,
                ticket: Zeroizing::new(
                    ticket
                        .as_slice()
                        .try_into()
                        .map_err(|_| ChatDeviceError::InvalidEncoding)?,
                ),
                endpoint,
            }));
        }
        V2ChatMessageContent::DeviceAdded {
            statement_account_id,
            encryption_public_key,
        } => {
            let device = PeerDevice {
                account_id: statement_account_id
                    .as_slice()
                    .try_into()
                    .map_err(|_| ChatDeviceError::InvalidEncoding)?,
                public_key: encryption_public_key
                    .as_slice()
                    .try_into()
                    .map_err(|_| ChatDeviceError::InvalidPeerKey)?,
            };
            validate_public_key(&device.public_key)?;
            DeviceLifecycle::Added(device)
        }
        V2ChatMessageContent::DeviceRemoved {
            statement_account_id,
        } => DeviceLifecycle::Removed(
            statement_account_id
                .as_slice()
                .try_into()
                .map_err(|_| ChatDeviceError::InvalidEncoding)?,
        ),
        V2ChatMessageContent::ChatAccepted { request_id } => {
            validate_id(&request_id)?;
            DeviceLifecycle::Accepted { request_id }
        }
        V2ChatMessageContent::MultiChatAccepted { request_id, device } => {
            validate_id(&request_id)?;
            validate_public_key(&device.encryption_public_key)?;
            DeviceLifecycle::MultiAccepted {
                request_id,
                device: PeerDevice {
                    account_id: device.statement_account_id,
                    public_key: device.encryption_public_key,
                },
            }
        }
        V2ChatMessageContent::ContactAdded => DeviceLifecycle::ContactAdded,
        V2ChatMessageContent::LeftChat => DeviceLifecycle::LeftChat,
        ordinary => {
            validate_ordinary(&ordinary)?;
            return Ok(OpenedDeviceMessage::Ordinary(core::mem::take(bytes)));
        }
    };
    Ok(OpenedDeviceMessage::DeviceControl(DeviceControl {
        message_id: message.message_id,
        timestamp: message.timestamp,
        content: control,
    }))
}

fn validate_ordinary(content: &V2ChatMessageContent) -> Result<(), ChatDeviceError> {
    match content {
        V2ChatMessageContent::Text(text) => validate_text(text),
        V2ChatMessageContent::RichText { text, attachments } => {
            if attachments.as_ref().is_some_and(|files| !files.is_empty()) {
                return Err(ChatDeviceError::ForbiddenContent);
            }
            text.as_deref().map_or(Ok(()), validate_text)
        }
        V2ChatMessageContent::Reply {
            message_id,
            text,
            attachments,
        }
        | V2ChatMessageContent::Edited {
            message_id,
            new_text: text,
            attachments,
        } => {
            if attachments.as_ref().is_some_and(|files| !files.is_empty()) {
                return Err(ChatDeviceError::ForbiddenContent);
            }
            validate_id(message_id)?;
            text.as_deref().map_or(Ok(()), validate_text)
        }
        V2ChatMessageContent::Reacted { message_id, emoji }
        | V2ChatMessageContent::ReactionRemoved { message_id, emoji } => {
            validate_id(message_id)?;
            if emoji.len() > MAX_EMOJI_BYTES {
                return Err(ChatDeviceError::LimitExceeded);
            }
            if emoji.trim().is_empty() || emoji.chars().any(char::is_control) {
                return Err(ChatDeviceError::InvalidEncoding);
            }
            Ok(())
        }
        // CompactedMessages includes a claim ticket for another message batch;
        // forwarding it would create an unclassified nested-content bypass.
        _ => Err(ChatDeviceError::UnsupportedContent),
    }
}

fn check_size(len: usize) -> Result<(), ChatDeviceError> {
    if len > MAX_CHAT_ENVELOPE_BYTES {
        Err(ChatDeviceError::LimitExceeded)
    } else {
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), ChatDeviceError> {
    if id.is_empty() {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    if id.len() > MAX_ID_BYTES {
        return Err(ChatDeviceError::LimitExceeded);
    }
    Ok(())
}

fn validate_text(text: &str) -> Result<(), ChatDeviceError> {
    if text.len() > MAX_TEXT_BYTES {
        Err(ChatDeviceError::LimitExceeded)
    } else {
        Ok(())
    }
}

fn check_canonical_key(key: &[u8; 32]) -> Result<(), ChatDeviceError> {
    let mut modulus = [0xff; 32];
    modulus[0] = 0xed;
    modulus[31] = 0x7f;
    if key.iter().rev().cmp(modulus.iter().rev()) != core::cmp::Ordering::Less {
        return Err(ChatDeviceError::InvalidPeerKey);
    }
    Ok(())
}

fn validate_public_key(key: &[u8; 32]) -> Result<(), ChatDeviceError> {
    check_canonical_key(key)?;
    // A fixed clamped scalar suffices to detect all low-order X25519 inputs.
    let _shared = Zeroizing::new(
        chat::x25519_shared_secret(&[0; 32], key).map_err(|_| ChatDeviceError::InvalidPeerKey)?,
    );
    Ok(())
}

fn random_nonce() -> Result<[u8; 12], ChatDeviceError> {
    let mut nonce = [0; 12];
    getrandom::getrandom(&mut nonce).map_err(|_| ChatDeviceError::RandomnessUnavailable)?;
    Ok(nonce)
}

// Borrowed SCALE preflights protect secret allocations made by the shared codec.
// The shared codec still owns message/transport decoding and all wire encoding.
fn take<'a>(input: &mut &'a [u8], len: usize) -> Result<&'a [u8], ChatDeviceError> {
    if input.len() < len {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    let (value, rest) = input.split_at(len);
    *input = rest;
    Ok(value)
}

fn count(input: &mut &[u8]) -> Result<usize, ChatDeviceError> {
    Compact::<u32>::decode(input)
        .map(|value| value.0 as usize)
        .map_err(|_| ChatDeviceError::InvalidEncoding)
}

fn data<'a>(input: &mut &'a [u8]) -> Result<&'a [u8], ChatDeviceError> {
    let len = count(input)?;
    take(input, len)
}

fn id(input: &mut &[u8]) -> Result<(), ChatDeviceError> {
    let value = core::str::from_utf8(data(input)?).map_err(|_| ChatDeviceError::InvalidEncoding)?;
    validate_id(value)
}

fn finish(input: &[u8]) -> Result<(), ChatDeviceError> {
    if input.is_empty() {
        Ok(())
    } else {
        Err(ChatDeviceError::InvalidEncoding)
    }
}

fn preflight_envelope(bytes: &[u8]) -> Result<(), ChatDeviceError> {
    check_size(bytes.len())?;
    let mut input = bytes;
    if !matches!(take(&mut input, 1)?[0], 2 | 3) {
        return Err(ChatDeviceError::UnsupportedContent);
    }
    if data(&mut input)?.len() < AEAD_OVERHEAD {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    let peers = count(&mut input)?;
    if peers == 0 || peers > MAX_CHAT_PEERS {
        return Err(ChatDeviceError::InvalidRoster);
    }
    for _ in 0..peers {
        take(&mut input, 32)?;
        if data(&mut input)?.len() != WRAPPED_KEY_BYTES {
            return Err(ChatDeviceError::InvalidEncoding);
        }
    }
    finish(input)
}

fn preflight_request(bytes: &[u8]) -> Result<(), ChatDeviceError> {
    check_size(bytes.len())?;
    let mut input = bytes;
    id(&mut input)?;
    let messages = count(&mut input)?;
    if messages > MAX_MESSAGES {
        return Err(ChatDeviceError::LimitExceeded);
    }
    for _ in 0..messages {
        if data(&mut input)?.is_empty() {
            return Err(ChatDeviceError::InvalidEncoding);
        }
    }
    finish(input)
}

fn decode_response(bytes: &[u8]) -> Result<chat::V2MessageExchangeResponse, ChatDeviceError> {
    let mut input = bytes;
    id(&mut input)?;
    take(&mut input, 1)?;
    finish(input)?;
    chat::decode_message_exchange_response_plaintext(bytes)
        .map_err(|_| ChatDeviceError::InvalidEncoding)
}

fn message_header(bytes: &[u8]) -> Result<(&[u8], u8, u64), ChatDeviceError> {
    check_size(bytes.len())?;
    let mut input = bytes;
    id(&mut input)?;
    let timestamp = u64::decode(&mut input).map_err(|_| ChatDeviceError::InvalidEncoding)?;
    if take(&mut input, 1)?[0] != 0 {
        return Err(ChatDeviceError::UnsupportedContent);
    }
    let index = take(&mut input, 1)?[0];
    Ok((input, index, timestamp))
}

fn preflight_payment(input: &mut &[u8]) -> Result<(), ChatDeviceError> {
    let amount = Compact::<u128>::decode(input)
        .map_err(|_| ChatDeviceError::InvalidEncoding)?
        .0;
    if amount == 0 {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    let keys = count(input)?;
    if keys == 0 || keys > input.len() / 66 {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    for _ in 0..keys {
        if data(input)?.len() != 64 {
            return Err(ChatDeviceError::InvalidEncoding);
        }
    }
    finish(input)
}

fn preflight_compaction(input: &mut &[u8]) -> Result<(), ChatDeviceError> {
    if data(input)?.len() != 32 || data(input)?.len() != 32 || take(input, 1)?[0] != 0 {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    let endpoint =
        core::str::from_utf8(data(input)?).map_err(|_| ChatDeviceError::InvalidEncoding)?;
    if !endpoint.starts_with("wss://") {
        return Err(ChatDeviceError::InvalidEncoding);
    }
    finish(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(value: u8) -> HostChatDevice {
        HostChatDevice::from_secret([value; 32], [value.wrapping_add(64); 32])
    }

    fn peer(device: &HostChatDevice) -> PeerDevice {
        PeerDevice {
            account_id: device.account_id,
            public_key: device.public_key(),
        }
    }

    // Construct independently through the shared native codec, not Host sealing.
    fn native_envelope(
        sender: &HostChatDevice,
        recipients: &[PeerDevice],
        body: &[u8],
        response: bool,
    ) -> Vec<u8> {
        let one_shot = Zeroizing::new([0x27; 32]);
        let encrypted =
            chat::encrypt_multi_device_payload_with_nonce(&one_shot, body, [0x31; 12]).unwrap();
        let devices_info = recipients
            .iter()
            .enumerate()
            .map(|(index, recipient)| chat::V2RequestDeviceInfo {
                statement_account_id: recipient.account_id,
                encrypted_key: chat::wrap_multi_device_key_with_nonce(
                    &sender.secret,
                    &recipient.public_key,
                    &one_shot,
                    [index as u8; 12],
                )
                .unwrap(),
            })
            .collect();
        if response {
            chat::encode_transport_multi_response_plaintext(&chat::V2MultiDeviceResponse {
                encrypted_response: encrypted,
                devices_info,
            })
            .unwrap()
        } else {
            chat::encode_transport_multi_request_plaintext(&chat::V2MultiDeviceRequest {
                encrypted_request: encrypted,
                devices_info,
            })
            .unwrap()
        }
    }

    fn payment() -> Vec<u8> {
        chat::encode_coinage_send_message("payment", 23, "1000", &[vec![0x51; 64]]).unwrap()
    }

    #[test]
    fn native_mixed_request_keeps_secrets_private_and_preserves_message_order() {
        let alice = device(1);
        let bob = device(2);
        let third = device(3);
        let ordinary = chat::encode_text_message("text", 21, "hello").unwrap();
        let control =
            chat::encode_device_added_message("add", 22, &third.account_id, &third.public_key())
                .unwrap();
        let messages = Zeroizing::new(vec![ordinary.clone(), control, payment()]);
        let body = Zeroizing::new(
            chat::encode_message_exchange_request_plaintext("request-ack", &messages).unwrap(),
        );
        let wire = native_envelope(&alice, &[peer(&bob)], &body, false);
        let OpenedDeviceExchange::Request {
            request_id,
            messages,
        } = bob.open_multi_device(&peer(&alice), &wire).unwrap()
        else {
            panic!("request became response")
        };
        assert_eq!(request_id, "request-ack");
        assert_eq!(messages.len(), 3);
        match &messages[0] {
            OpenedDeviceMessage::Ordinary(bytes) => assert_eq!(bytes, &ordinary),
            _ => panic!("ordinary message misclassified"),
        }
        match &messages[1] {
            OpenedDeviceMessage::DeviceControl(control) => {
                assert_eq!(control.message_id, "add");
                assert_eq!(control.timestamp, 22);
                assert!(matches!(control.content, DeviceLifecycle::Added(found)
                    if found == peer(&third)));
            }
            _ => panic!("control escaped classification"),
        }
        match &messages[2] {
            OpenedDeviceMessage::Payment(memo) => {
                assert_eq!(memo.message_id, "payment");
                assert_eq!(memo.timestamp, 23);
                assert_eq!(memo.total_value, 1000);
                assert_eq!(memo.coin_keys.as_slice(), &[vec![0x51; 64]]);
            }
            _ => panic!("payment escaped classification"),
        }
    }

    #[test]
    fn host_sealing_remains_readable_by_native_codec_for_each_recipient() {
        let alice = device(1);
        let bob = device(2);
        let carol = device(3);
        let messages = Zeroizing::new(vec![payment()]);
        let plaintext = Zeroizing::new(
            chat::encode_transport_request_plaintext("native-ack", &messages).unwrap(),
        );
        let wire = alice
            .seal_multi_device(&[peer(&bob), peer(&carol)], &plaintext)
            .unwrap();
        let other = alice
            .seal_multi_device(&[peer(&bob), peer(&carol)], &plaintext)
            .unwrap();
        assert_ne!(wire, other, "one-shot keys and nonces must be fresh");
        let V2StatementTransportData::MultiRequest(request) =
            chat::decode_transport_plaintext(&wire).unwrap()
        else {
            panic!("non-native transport kind")
        };
        for recipient in [&bob, &carol] {
            let entry = request
                .devices_info
                .iter()
                .find(|entry| entry.statement_account_id == recipient.account_id)
                .unwrap();
            let key = Zeroizing::new(
                chat::unwrap_multi_device_key(
                    &recipient.secret,
                    &alice.public_key(),
                    &entry.encrypted_key,
                )
                .unwrap(),
            );
            let decrypted = Zeroizing::new(
                chat::decrypt_multi_device_payload(&key, &request.encrypted_request).unwrap(),
            );
            assert_eq!(
                &decrypted[..],
                &plaintext[1..],
                "native inner has no transport tag"
            );
        }
    }

    #[test]
    fn request_response_disambiguation_preserves_native_ack_codes() {
        let alice = device(1);
        let bob = device(2);
        for code in [0, 255] {
            let response = chat::encode_transport_response_plaintext("ack", code).unwrap();
            let wire = alice.seal_multi_device(&[peer(&bob)], &response).unwrap();
            assert_eq!(wire[0], 3);
            assert!(
                matches!(bob.open_multi_device(&peer(&alice), &wire).unwrap(),
                OpenedDeviceExchange::Response { request_id, response_code }
                    if request_id == "ack" && response_code == code)
            );
        }
        let empty_request = chat::encode_transport_request_plaintext("ack", &[]).unwrap();
        let wire = alice
            .seal_multi_device(&[peer(&bob)], &empty_request)
            .unwrap();
        assert_eq!(wire[0], 2);
        assert!(
            matches!(bob.open_multi_device(&peer(&alice), &wire).unwrap(),
            OpenedDeviceExchange::Request { request_id, messages }
                if request_id == "ack" && messages.is_empty())
        );
    }

    #[test]
    fn invalid_keys_and_roster_aliases_fail_closed() {
        let alice = device(1);
        let bob = device(2);
        let plaintext = chat::encode_transport_request_plaintext("ack", &[]).unwrap();
        let wire = alice.seal_multi_device(&[peer(&bob)], &plaintext).unwrap();
        let mut modulus = [0xff; 32];
        modulus[0] = 0xed;
        modulus[31] = 0x7f;
        let mut high_bit = bob.public_key();
        high_bit[31] |= 0x80;
        let mut one = [0; 32];
        one[0] = 1;
        for public_key in [[0; 32], one, modulus, high_bit] {
            let invalid = PeerDevice {
                account_id: [9; 32],
                public_key,
            };
            assert_eq!(
                alice.seal_multi_device(&[invalid], &plaintext),
                Err(ChatDeviceError::InvalidPeerKey)
            );
            assert!(matches!(
                bob.open_multi_device(&invalid, &wire),
                Err(ChatDeviceError::InvalidPeerKey)
            ));
        }
        for roster in [
            vec![peer(&bob), peer(&bob)],
            vec![
                peer(&bob),
                PeerDevice {
                    account_id: [9; 32],
                    ..peer(&bob)
                },
            ],
            vec![
                peer(&bob),
                PeerDevice {
                    public_key: device(3).public_key(),
                    ..peer(&bob)
                },
            ],
        ] {
            assert_eq!(
                alice.seal_multi_device(&roster, &plaintext),
                Err(ChatDeviceError::InvalidRoster)
            );
        }
        let alias = PeerDevice {
            account_id: alice.account_id,
            public_key: bob.public_key(),
        };
        assert_eq!(
            alice.seal_multi_device(&[alias], &plaintext),
            Err(ChatDeviceError::InvalidPeerKey)
        );
    }

    #[test]
    fn wrong_sender_recipient_and_duplicate_recipient_are_rejected() {
        let alice = device(1);
        let bob = device(2);
        let carol = device(3);
        let plaintext = chat::encode_transport_request_plaintext("ack", &[]).unwrap();
        let wire = alice.seal_multi_device(&[peer(&bob)], &plaintext).unwrap();
        assert!(matches!(
            bob.open_multi_device(&peer(&carol), &wire),
            Err(ChatDeviceError::AuthenticationFailed)
        ));
        assert!(matches!(
            carol.open_multi_device(&peer(&alice), &wire),
            Err(ChatDeviceError::WrongRecipient)
        ));
        let wrong_key = HostChatDevice::from_secret(bob.account_id, [99; 32]);
        assert!(matches!(
            wrong_key.open_multi_device(&peer(&alice), &wire),
            Err(ChatDeviceError::AuthenticationFailed)
        ));
        let mut input = match chat::decode_transport_plaintext(&wire).unwrap() {
            V2StatementTransportData::MultiRequest(input) => input,
            _ => panic!("wrong envelope kind"),
        };
        input.devices_info.push(input.devices_info[0].clone());
        let duplicated = chat::encode_transport_multi_request_plaintext(&input).unwrap();
        assert!(matches!(
            bob.open_multi_device(&peer(&alice), &duplicated),
            Err(ChatDeviceError::InvalidRoster)
        ));
        input.devices_info.pop();
        input.encrypted_request[12] ^= 1;
        let tampered = chat::encode_transport_multi_request_plaintext(&input).unwrap();
        assert!(matches!(
            bob.open_multi_device(&peer(&alice), &tampered),
            Err(ChatDeviceError::AuthenticationFailed)
        ));
    }

    #[test]
    fn guest_cannot_inject_payments_lifecycle_or_compacted_batches() {
        let added = device(3);
        let forbidden = [
            payment(),
            chat::encode_device_added_message("add", 1, &added.account_id, &added.public_key())
                .unwrap(),
            chat::encode_device_removed_message("remove", 1, &added.account_id).unwrap(),
            chat::encode_chat_accepted_message("accept", 1, "request").unwrap(),
            chat::encode_multi_chat_accepted_message(
                "multi",
                1,
                "request",
                &chat::V2PeerDevice {
                    statement_account_id: added.account_id,
                    encryption_public_key: added.public_key(),
                },
            )
            .unwrap(),
            chat::encode_contact_added_message("contact", 1).unwrap(),
            chat::encode_left_chat_message("left", 1).unwrap(),
        ];
        let text = chat::encode_text_message("text", 1, "ordinary first").unwrap();
        for message in forbidden {
            assert_eq!(
                validate_guest_messages(&[text.clone(), message]),
                Err(ChatDeviceError::ForbiddenContent)
            );
        }
        let compacted = chat::encode_compacted_messages_message(
            "batch",
            1,
            &[1; 32],
            &[2; 32],
            &chat::V2NodeEndpoint::WssUrl("wss://example.invalid".into()),
        )
        .unwrap();
        assert_eq!(
            validate_guest_messages(&[compacted]),
            Err(ChatDeviceError::UnsupportedContent)
        );
    }

    #[test]
    fn guest_ordinary_operations_are_parsed_fully_and_bounded() {
        let ordinary = vec![
            chat::encode_text_message("text", 1, "hello").unwrap(),
            chat::encode_rich_text_message("rich", 2, Some("rich text"), None).unwrap(),
            chat::encode_reply_message("reply", 3, "text", Some("reply")).unwrap(),
            chat::encode_edited_message("edit", 4, "text", Some("edited")).unwrap(),
            chat::encode_reacted_message("react", 5, "text", "ok").unwrap(),
            chat::encode_reaction_removed_message("remove", 6, "text", "ok").unwrap(),
        ];
        assert_eq!(validate_guest_messages(&ordinary), Ok(()));
        let mut nested = chat::encode_reply_message("nested", 7, "text", Some("body")).unwrap();
        *nested.last_mut().unwrap() = 1; // Unsupported attachments, not ordinary rich text.
        nested.extend_from_slice(&payment());
        assert_eq!(
            validate_guest_messages(&[nested]),
            Err(ChatDeviceError::InvalidEncoding)
        );
        let too_long =
            chat::encode_text_message("long", 1, &"x".repeat(MAX_TEXT_BYTES + 1)).unwrap();
        assert_eq!(
            validate_guest_messages(&[too_long]),
            Err(ChatDeviceError::LimitExceeded)
        );
        let mut trailing = ordinary[0].clone();
        trailing.extend_from_slice(&payment());
        assert_eq!(
            validate_guest_messages(&[trailing]),
            Err(ChatDeviceError::InvalidEncoding)
        );
    }

    #[test]
    fn unknown_and_malformed_secret_content_never_returns_partial_ordinary_data() {
        let alice = device(1);
        let bob = device(2);
        let ordinary = chat::encode_text_message("text", 1, "safe").unwrap();
        let mut unknown = ordinary.clone();
        let (content, _, _) = message_header(&unknown).unwrap();
        let content_offset = unknown.len() - content.len();
        unknown[content_offset - 1] = 254;
        let mut unsupported_version = ordinary.clone();
        unsupported_version[content_offset - 2] = 1;
        let mut truncated_payment = payment();
        truncated_payment.pop();
        let mut trailing_payment = payment();
        trailing_payment.push(0);
        let wrong_key_length =
            chat::encode_coinage_send_message("bad-key", 2, "1", &[vec![0x33; 63]]).unwrap();
        for invalid in [
            unknown,
            unsupported_version,
            truncated_payment,
            trailing_payment,
            wrong_key_length,
        ] {
            assert!(validate_guest_messages(&[invalid.clone()]).is_err());
            let messages = Zeroizing::new(vec![ordinary.clone(), payment(), invalid]);
            let body = Zeroizing::new(
                chat::encode_message_exchange_request_plaintext("ack", &messages).unwrap(),
            );
            let wire = native_envelope(&alice, &[peer(&bob)], &body, false);
            assert!(bob.open_multi_device(&peer(&alice), &wire).is_err());
        }
    }

    #[test]
    fn outer_and_inner_framing_and_resource_limits_are_strict() {
        let alice = device(1);
        let bob = device(2);
        let plaintext = chat::encode_transport_request_plaintext("ack", &[]).unwrap();
        let mut wire = alice.seal_multi_device(&[peer(&bob)], &plaintext).unwrap();
        wire.push(0);
        assert!(matches!(
            bob.open_multi_device(&peer(&alice), &wire),
            Err(ChatDeviceError::InvalidEncoding)
        ));
        assert!(matches!(
            bob.open_multi_device(&peer(&alice), &plaintext),
            Err(ChatDeviceError::UnsupportedContent)
        ));
        let peers = (1..=MAX_CHAT_PEERS + 1)
            .map(|value| peer(&device(value as u8 + 1)))
            .collect::<Vec<_>>();
        assert_eq!(
            alice.seal_multi_device(&peers, &plaintext),
            Err(ChatDeviceError::InvalidRoster)
        );
        let oversized = vec![0; MAX_CHAT_ENVELOPE_BYTES + 1];
        assert!(matches!(
            bob.open_multi_device(&peer(&alice), &oversized),
            Err(ChatDeviceError::LimitExceeded)
        ));
        let body = Zeroizing::new(
            chat::encode_message_exchange_request_plaintext("ack", &[payment()]).unwrap(),
        );
        for malformed in [
            [&[0x0d, 0x00][..], &body[1..]].concat(), // Noncanonical compact string length.
            [&body[..], &[0][..]].concat(),           // Trailing inner bytes.
            body[..body.len() - 1].to_vec(),          // Truncated secret-bearing message.
        ] {
            let malformed = Zeroizing::new(malformed);
            let wire = native_envelope(&alice, &[peer(&bob)], &malformed, false);
            assert!(matches!(
                bob.open_multi_device(&peer(&alice), &wire),
                Err(ChatDeviceError::InvalidEncoding)
            ));
        }
    }
}
