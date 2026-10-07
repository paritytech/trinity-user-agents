//! Authenticated, endpoint-bound Media envelopes. This is not a product API.
//!
//! The authority supplies the canonical product's Index(0) account; its signature
//! is deliberately separate from the ephemeral endpoint's packet signatures.
//! Keep `RuntimeSecrets` for the entire runtime, including reconnects. Dropping
//! it destroys the receiver keys and replay state together. Never route a retired
//! endpoint's inbox to its replacement. Authentication alone is not admission:
//! call `RuntimeSecrets::admit_replay` before any effect of an opened packet.

use std::{collections::BTreeMap, time::Duration};

use hpke::{
    Deserializable, Kem, OpModeR, OpModeS, Serializable,
    aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use parity_scale_codec::{Compact, Decode, Encode};
use rand_core::{OsRng, RngCore};
use schnorrkel::{ExpansionMode, MiniSecretKey, PublicKey, Signature};
use crate::platform::normalize_product_identifier;
use zeroize::Zeroizing;

/// This context cannot be reached through substrate-context raw signing.
pub(crate) const ACCOUNT_SIGNING_CONTEXT: &[u8] = b"truapi-media-account-v1";
const PACKET_SIGNING_CONTEXT: &[u8] = b"truapi-media-packet-v1";
const ADVERTISEMENT_PREFIX: &[u8] = b"truapi/media/advertisement/v1";
const ADVERTISEMENT_TOPIC_PREFIX: &[u8] = b"truapi/media/advertisements/v1";
const INBOX_TOPIC_PREFIX: &[u8] = b"truapi/media/inbox/v1";
const PACKET_PREFIX: &[u8] = b"truapi/media/packet/v1";
const HPKE_PREFIX: &[u8] = b"truapi/media/hpke/v1";
const VERSION: u16 = 1;
pub(crate) const MAX_PRODUCT_ID_BYTES: usize = 255;
pub(crate) const MAX_ADVERTISEMENT_LIFETIME: u64 = 600;
pub(crate) const MAX_PACKET_LIFETIME: u64 = 60;
pub(crate) const CLOCK_SKEW: u64 = 30;
pub(crate) const MAX_PACKET_BYTES: usize = 128 * 1024;
pub(crate) const MAX_PLAINTEXT_BYTES: usize = 96 * 1024;
const TAG_BYTES: usize = 16;
// Fixed fields, a two-byte compact length, and the maximum product identifier.
pub(crate) const MAX_UNSIGNED_ADVERTISEMENT_BYTES: usize = 2 + 32 * 5 + 2 + MAX_PRODUCT_ID_BYTES + 16;
pub(crate) const MAX_ADVERTISEMENT_BYTES: usize = MAX_UNSIGNED_ADVERTISEMENT_BYTES + 64;

type MediaKem = X25519HkdfSha256;
type EncryptionSecret = <MediaKem as Kem>::PrivateKey;

/// Finite, input-independent diagnostics; never carry wire bytes or secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum MediaProtocolError {
    #[error("invalid media encoding")]
    InvalidEncoding,
    #[error("unsupported media protocol version")]
    UnsupportedVersion,
    #[error("invalid media product identifier")]
    InvalidProduct,
    #[error("invalid media key")]
    InvalidKey,
    #[error("media identity does not match")]
    WrongIdentity,
    #[error("media endpoint does not match")]
    WrongEndpoint,
    #[error("invalid media lifetime")]
    InvalidLifetime,
    #[error("media timestamp is in the future")]
    Future,
    #[error("media authorization has expired")]
    Expired,
    #[error("media timestamp overflow")]
    TimeOverflow,
    #[error("invalid media signature")]
    InvalidSignature,
    #[error("media size limit exceeded")]
    SizeLimit,
    #[error("media encryption failed")]
    Encryption,
    #[error("media decryption failed")]
    Decryption,
    #[error("media entropy unavailable")]
    EntropyUnavailable,
    #[error("duplicate media message")]
    Replay,
    #[error("media replay capacity exhausted")]
    ReplayFull,
    #[error("invalid media replay capacity")]
    InvalidReplayCapacity,
    #[error("media clock moved backward")]
    ClockRegression,
}

type Result<T, E = MediaProtocolError> = std::result::Result<T, E>;

/// Authority-derived route, not a product-selected signing identity.
#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
#[debug("MediaIdentity(<redacted>)")]
pub(crate) struct MediaIdentity {
    pub network: [u8; 32],
    pub product_id: String,
    pub account: [u8; 32],
}

impl MediaIdentity {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_product(&self.product_id)?;
        validate_signing_key(&self.account)?;
        Ok(())
    }
}

/// Exact unsigned advertisement fields in protocol order.
#[derive(Clone, PartialEq, Eq, Encode, derive_more::Debug)]
#[debug("UnsignedAdvertisement(<redacted>)")]
pub struct UnsignedAdvertisement {
    pub version: u16,
    pub network: [u8; 32],
    pub product_id: String,
    pub account: [u8; 32],
    pub endpoint_id: [u8; 32],
    pub encryption_key: [u8; 32],
    pub signing_key: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
}

impl UnsignedAdvertisement {
    /// Sign these exact bytes under `ACCOUNT_SIGNING_CONTEXT`, using the
    /// authority-derived Index(0) key. No endpoint secret can authorize them.
    pub(crate) fn account_signing_input(&self) -> Vec<u8> {
        domain_encoded(ADVERTISEMENT_PREFIX, self)
    }

    /// Attach an independently obtained account signature, checking it before
    /// returning a certificate usable for encryption.
    pub(crate) fn authenticate(
        self,
        signature: [u8; 64],
        now: u64,
    ) -> Result<VerifiedAdvertisement> {
        let advertisement = MediaAdvertisement { fields: self, signature };
        verify_advertisement(&advertisement, now)?;
        Ok(VerifiedAdvertisement(advertisement))
    }

    fn identity(&self) -> MediaIdentity {
        MediaIdentity {
            network: self.network,
            product_id: self.product_id.clone(),
            account: self.account,
        }
    }

    fn matches_identity(&self, identity: &MediaIdentity) -> bool {
        self.network == identity.network
            && self.product_id == identity.product_id
            && self.account == identity.account
    }
}

/// The nested fields encode exactly as the specified flat SCALE record.
/// There is intentionally no unbounded `Decode` implementation.
#[derive(Clone, PartialEq, Eq, Encode, derive_more::Debug)]
#[debug("MediaAdvertisement(<redacted>)")]
pub(crate) struct MediaAdvertisement {
    pub fields: UnsignedAdvertisement,
    pub signature: [u8; 64],
}

/// Authenticated certificate; immutable after validation. Time validity is
/// checked again at every seal/open, since a cached certificate can expire.
#[derive(Clone, derive_more::Debug)]
#[debug("VerifiedAdvertisement(<redacted>)")]
pub(crate) struct VerifiedAdvertisement(MediaAdvertisement);

impl VerifiedAdvertisement {
    pub(crate) fn advertisement(&self) -> &MediaAdvertisement {
        &self.0
    }

    pub(crate) fn identity(&self) -> MediaIdentity {
        self.0.fields.identity()
    }
}

/// Not cloneable, serializable, or exportable. HPKE's X25519 private key and
/// schnorrkel's keypair zeroize on drop; temporary key material is zeroized too.
/// Replay state has no reset API and cannot outlive these particular keys.
#[derive(derive_more::Debug)]
#[debug("RuntimeSecrets(<redacted>)")]
pub(crate) struct RuntimeSecrets {
    identity: MediaIdentity,
    endpoint_id: [u8; 32],
    encryption_secret: EncryptionSecret,
    encryption_key: [u8; 32],
    signing: schnorrkel::Keypair,
    replay: ReplayWindow,
}

impl RuntimeSecrets {
    pub(crate) fn generate(identity: MediaIdentity, replay_capacity: usize) -> Result<Self> {
        identity.validate()?;
        if replay_capacity == 0 {
            return Err(MediaProtocolError::InvalidReplayCapacity);
        }
        let mut seed = Zeroizing::new([0; 32]);
        OsRng.try_fill_bytes(&mut *seed).map_err(|_| MediaProtocolError::EntropyUnavailable)?;
        let (encryption_secret, encryption_public) = MediaKem::derive_keypair(&*seed);
        OsRng.try_fill_bytes(&mut *seed).map_err(|_| MediaProtocolError::EntropyUnavailable)?;
        let signing = MiniSecretKey::from_bytes(&*seed)
            .map_err(|_| MediaProtocolError::InvalidKey)?
            .expand_to_keypair(ExpansionMode::Ed25519);
        Ok(Self {
            identity,
            endpoint_id: random_id()?,
            encryption_secret,
            encryption_key: encryption_public.to_bytes().into(),
            signing,
            replay: ReplayWindow { capacity: replay_capacity, entries: BTreeMap::new(), last_now: 0 },
        })
    }

    /// Renewal changes only the signed lifetime, never the runtime's identity,
    /// endpoint, or encryption/signing keys.
    pub(crate) fn unsigned_advertisement(
        &self,
        issued_at: u64,
        expires_at: u64,
    ) -> Result<UnsignedAdvertisement> {
        validate_lifetime(issued_at, expires_at, MAX_ADVERTISEMENT_LIFETIME)?;
        Ok(UnsignedAdvertisement {
            version: VERSION,
            network: self.identity.network,
            product_id: self.identity.product_id.clone(),
            account: self.identity.account,
            endpoint_id: self.endpoint_id,
            encryption_key: self.encryption_key,
            signing_key: self.signing.public.to_bytes(),
            issued_at,
            expires_at,
        })
    }

    /// Separate admission step after authentication and before all effects.
    /// The opened token is tied to the receiver epoch and cannot be forged or
    /// modified through this module's API.
    pub(crate) fn admit_replay(&mut self, packet: &OpenedPacket, now: u64) -> Result<()> {
        if packet.recipient_endpoint != self.endpoint_id {
            return Err(MediaProtocolError::WrongEndpoint);
        }
        self.replay.admit(
            ReplayKey {
                account: packet.sender.0.fields.account,
                endpoint: packet.sender.0.fields.endpoint_id,
                message: packet.message_id,
            },
            packet.expires_at,
            now,
        )
    }

    fn validate_local(&self, advertisement: &VerifiedAdvertisement, now: u64) -> Result<()> {
        let fields = &advertisement.0.fields;
        if !fields.matches_identity(&self.identity) {
            return Err(MediaProtocolError::WrongIdentity);
        }
        if fields.endpoint_id != self.endpoint_id
            || fields.encryption_key != self.encryption_key
            || fields.signing_key != self.signing.public.to_bytes()
        {
            return Err(MediaProtocolError::WrongEndpoint);
        }
        validate_time(fields.issued_at, fields.expires_at, MAX_ADVERTISEMENT_LIFETIME, now)
    }
}

/// Authority-side certification gate before signing: the expected identity
/// must be independently derived from the canonical product and Index(0).
pub(crate) fn validate_unsigned_advertisement(
    fields: &UnsignedAdvertisement,
    expected: &MediaIdentity,
    now: u64,
) -> Result<()> {
    expected.validate()?;
    if !fields.matches_identity(expected) {
        return Err(MediaProtocolError::WrongIdentity);
    }
    validate_unsigned_fields(fields, now)?;
    Ok(())
}

/// Bounded canonical SCALE input for the host-private certification request.
/// Signing authorities must never decode it using an unbounded derived codec.
pub(crate) fn decode_unsigned_advertisement(
    bytes: &[u8],
    expected: &MediaIdentity,
    now: u64,
) -> Result<UnsignedAdvertisement> {
    if bytes.len() > MAX_UNSIGNED_ADVERTISEMENT_BYTES {
        return Err(MediaProtocolError::SizeLimit);
    }
    let mut input = bytes;
    let fields = decode_unsigned_fields(&mut input)?;
    if !input.is_empty() {
        return Err(MediaProtocolError::InvalidEncoding);
    }
    validate_unsigned_advertisement(&fields, expected, now)?;
    Ok(fields)
}

/// Strict, size-bounded SCALE decoding followed by exact route and account
/// authentication. Expected identity must come from trusted peer resolution.
pub(crate) fn decode_advertisement(
    bytes: &[u8],
    expected: &MediaIdentity,
    now: u64,
) -> Result<VerifiedAdvertisement> {
    if bytes.len() > MAX_ADVERTISEMENT_BYTES {
        return Err(MediaProtocolError::SizeLimit);
    }
    expected.validate()?;
    let mut input = bytes;
    let advertisement = decode_advertisement_fields(&mut input)?;
    if !input.is_empty() {
        return Err(MediaProtocolError::InvalidEncoding);
    }
    if !advertisement.fields.matches_identity(expected) {
        return Err(MediaProtocolError::WrongIdentity);
    }
    verify_advertisement(&advertisement, now)?;
    Ok(VerifiedAdvertisement(advertisement))
}

pub(crate) fn advertisement_topic(identity: &MediaIdentity) -> Result<[u8; 32]> {
    identity.validate()?;
    Ok(sp_crypto_hashing::blake2_256(&domain_encoded(
        ADVERTISEMENT_TOPIC_PREFIX,
        &(&identity.network, identity.product_id.as_str(), &identity.account),
    )))
}

pub(crate) fn inbox_topic(identity: &MediaIdentity, endpoint_id: &[u8; 32]) -> Result<[u8; 32]> {
    identity.validate()?;
    nonzero(endpoint_id)?;
    Ok(sp_crypto_hashing::blake2_256(&domain_encoded(
        INBOX_TOPIC_PREFIX,
        &(&identity.network, identity.product_id.as_str(), &identity.account, endpoint_id),
    )))
}

#[derive(Encode, derive_more::Debug)]
#[debug("PacketHeader(<redacted>)")]
struct PacketHeader {
    version: u16,
    sender: MediaAdvertisement,
    recipient_account: [u8; 32],
    recipient_endpoint: [u8; 32],
    message_id: [u8; 32],
    issued_at: u64,
    expires_at: u64,
}

#[derive(Encode, derive_more::Debug)]
#[debug("SealedPacket(<redacted>)")]
struct SealedPacket<'a> {
    header: PacketHeader,
    encapsulated_key: [u8; 32],
    ciphertext: &'a [u8],
    signature: [u8; 64],
}

impl SealedPacket<'_> {
    fn signing_input(&self) -> Vec<u8> {
        domain_encoded(PACKET_PREFIX, &(&self.header, &self.encapsulated_key, &self.ciphertext))
    }
}

/// Seal only to an authenticated, current certificate on the same product and
/// network. A new message ID and RFC 9180 base-mode encapsulation are generated
/// for each packet. Lifetime is explicit, never silently clamped.
pub(crate) fn seal_packet(
    keys: &RuntimeSecrets,
    sender: &VerifiedAdvertisement,
    recipient: &VerifiedAdvertisement,
    plaintext: &[u8],
    now: u64,
    expires_at: u64,
) -> Result<Vec<u8>> {
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(MediaProtocolError::SizeLimit);
    }
    keys.validate_local(sender, now)?;
    let recipient_fields = &recipient.0.fields;
    validate_route(recipient_fields, &keys.identity)?;
    validate_time(recipient_fields.issued_at, recipient_fields.expires_at, MAX_ADVERTISEMENT_LIFETIME, now)?;
    validate_time(now, expires_at, MAX_PACKET_LIFETIME, now)?;
    if expires_at > sender.0.fields.expires_at || expires_at > recipient_fields.expires_at {
        return Err(MediaProtocolError::InvalidLifetime);
    }
    let header = PacketHeader {
        version: VERSION,
        sender: sender.0.clone(),
        recipient_account: recipient_fields.account,
        recipient_endpoint: recipient_fields.endpoint_id,
        message_id: random_id()?,
        issued_at: now,
        expires_at,
    };
    let recipient_key = <MediaKem as Kem>::PublicKey::from_bytes(&recipient_fields.encryption_key)
        .map_err(|_| MediaProtocolError::InvalidKey)?;
    let (encapsulated_key, ciphertext) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, MediaKem, _>(
        &OpModeS::Base,
        &recipient_key,
        &hpke_info(&header),
        plaintext,
        &header.encode(),
        &mut OsRng,
    ).map_err(|_| MediaProtocolError::Encryption)?;
    let packet = SealedPacket {
        header,
        encapsulated_key: encapsulated_key.to_bytes().into(),
        ciphertext: &ciphertext,
        signature: [0; 64],
    };
    let mut bytes = packet.signing_input();
    let signature = keys.signing.sign_simple(PACKET_SIGNING_CONTEXT, &bytes).to_bytes();
    // Reuse the signature input's allocation as the wire packet.
    bytes.drain(..PACKET_PREFIX.len());
    signature.encode_to(&mut bytes);
    if bytes.len() > MAX_PACKET_BYTES {
        return Err(MediaProtocolError::SizeLimit);
    }
    Ok(bytes)
}

/// Private authenticated plaintext, still requiring replay admission. Getters
/// preserve the admission token's identity, expiry, and message ID unchanged.
#[derive(derive_more::Debug)]
#[debug("OpenedPacket(<redacted>)")]
pub(crate) struct OpenedPacket {
    sender: VerifiedAdvertisement,
    message_id: [u8; 32],
    expires_at: u64,
    plaintext: Zeroizing<Vec<u8>>,
    recipient_endpoint: [u8; 32],
}

impl OpenedPacket {
    #[cfg(test)]
    pub(crate) fn sender(&self) -> &VerifiedAdvertisement { &self.sender }
    pub(crate) fn expires_at(&self) -> u64 { self.expires_at }
    #[cfg(test)]
    pub(crate) fn plaintext(&self) -> &[u8] { &self.plaintext }

    /// Consume only after `RuntimeSecrets::admit_replay` succeeds. Transfers the
    /// private plaintext allocation without copying or removing zeroization.
    pub(crate) fn into_parts(self) -> (VerifiedAdvertisement, [u8; 32], u64, Zeroizing<Vec<u8>>) {
        (self.sender, self.message_id, self.expires_at, self.plaintext)
    }
}

/// All envelope checks and signatures precede decryption. No unsealed fallback.
/// The caller must not turn plaintext into effects until replay admission wins.
pub(crate) fn open_packet(
    keys: &RuntimeSecrets,
    local: &VerifiedAdvertisement,
    bytes: &[u8],
    now: u64,
) -> Result<OpenedPacket> {
    keys.validate_local(local, now)?;
    let packet = decode_packet(bytes)?;
    let header = &packet.header;
    if header.version != VERSION {
        return Err(MediaProtocolError::UnsupportedVersion);
    }
    if header.recipient_account != keys.identity.account {
        return Err(MediaProtocolError::WrongIdentity);
    }
    if header.recipient_endpoint != keys.endpoint_id {
        return Err(MediaProtocolError::WrongEndpoint);
    }
    validate_route(&header.sender.fields, &keys.identity)?;
    validate_time(header.issued_at, header.expires_at, MAX_PACKET_LIFETIME, now)?;
    if header.expires_at > header.sender.fields.expires_at
        || header.expires_at > local.0.fields.expires_at
    {
        return Err(MediaProtocolError::InvalidLifetime);
    }
    nonzero(&header.message_id)?;
    verify_advertisement(&header.sender, now)?;
    let signing_key = validate_signing_key(&header.sender.fields.signing_key)?;
    let signature = Signature::from_bytes(&packet.signature)
        .map_err(|_| MediaProtocolError::InvalidSignature)?;
    signing_key.verify_simple(PACKET_SIGNING_CONTEXT, &packet.signing_input(), &signature)
        .map_err(|_| MediaProtocolError::InvalidSignature)?;
    validate_encryption_key(&packet.encapsulated_key)?;
    let encapsulated_key = <MediaKem as Kem>::EncappedKey::from_bytes(&packet.encapsulated_key)
        .map_err(|_| MediaProtocolError::InvalidKey)?;
    let plaintext = Zeroizing::new(hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, MediaKem>(
        &OpModeR::Base,
        &keys.encryption_secret,
        &encapsulated_key,
        &hpke_info(header),
        packet.ciphertext,
        &header.encode(),
    ).map_err(|_| MediaProtocolError::Decryption)?);
    Ok(OpenedPacket {
        sender: VerifiedAdvertisement(packet.header.sender),
        message_id: packet.header.message_id,
        expires_at: packet.header.expires_at,
        plaintext,
        recipient_endpoint: packet.header.recipient_endpoint,
    })
}

fn hpke_info(header: &PacketHeader) -> Vec<u8> {
    let sender = &header.sender.fields;
    domain_encoded(HPKE_PREFIX, &(
        &sender.network,
        sender.product_id.as_str(),
        &sender.account,
        &sender.endpoint_id,
        &header.recipient_account,
        &header.recipient_endpoint,
    ))
}

fn domain_encoded(prefix: &[u8], value: &impl Encode) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(prefix.len() + value.size_hint());
    bytes.extend_from_slice(prefix);
    value.encode_to(&mut bytes);
    bytes
}

fn verify_advertisement(advertisement: &MediaAdvertisement, now: u64) -> Result<()> {
    let fields = &advertisement.fields;
    let account = validate_unsigned_fields(fields, now)?;
    let signature = Signature::from_bytes(&advertisement.signature)
        .map_err(|_| MediaProtocolError::InvalidSignature)?;
    account.verify_simple(ACCOUNT_SIGNING_CONTEXT, &fields.account_signing_input(), &signature)
        .map_err(|_| MediaProtocolError::InvalidSignature)
}

fn validate_unsigned_fields(fields: &UnsignedAdvertisement, now: u64) -> Result<PublicKey> {
    if fields.version != VERSION {
        return Err(MediaProtocolError::UnsupportedVersion);
    }
    validate_product(&fields.product_id)?;
    nonzero(&fields.endpoint_id)?;
    let account = validate_signing_key(&fields.account)?;
    validate_signing_key(&fields.signing_key)?;
    validate_encryption_key(&fields.encryption_key)?;
    validate_time(fields.issued_at, fields.expires_at, MAX_ADVERTISEMENT_LIFETIME, now)?;
    Ok(account)
}

fn validate_product(product: &str) -> Result<()> {
    if product.is_empty() || product.len() > MAX_PRODUCT_ID_BYTES {
        return Err(MediaProtocolError::InvalidProduct);
    }
    let normalized = normalize_product_identifier(product).map_err(|_| MediaProtocolError::InvalidProduct)?;
    if normalized != product {
        return Err(MediaProtocolError::InvalidProduct);
    }
    Ok(())
}

fn validate_route(fields: &UnsignedAdvertisement, identity: &MediaIdentity) -> Result<()> {
    if fields.network != identity.network || fields.product_id != identity.product_id {
        return Err(MediaProtocolError::WrongIdentity);
    }
    Ok(())
}

fn nonzero(bytes: &[u8; 32]) -> Result<()> {
    if bytes == &[0; 32] { Err(MediaProtocolError::InvalidKey) } else { Ok(()) }
}

fn validate_signing_key(bytes: &[u8; 32]) -> Result<PublicKey> {
    nonzero(bytes)?;
    PublicKey::from_bytes(bytes).map_err(|_| MediaProtocolError::InvalidKey)
}

fn validate_encryption_key(bytes: &[u8; 32]) -> Result<()> {
    // This scalar is public, used only to detect non-contributory (low-order)
    // public keys, including their RFC 7748 aliases. Preserve RFC 7748's
    // permissive field decoding; HPKE stays inside the maintained crate.
    let validation_scalar = x25519_dalek::StaticSecret::from([0x42; 32]);
    if !validation_scalar.diffie_hellman(&x25519_dalek::PublicKey::from(*bytes)).was_contributory() {
        return Err(MediaProtocolError::InvalidKey);
    }
    Ok(())
}

fn random_id() -> Result<[u8; 32]> {
    let mut id = [0; 32];
    OsRng.try_fill_bytes(&mut id).map_err(|_| MediaProtocolError::EntropyUnavailable)?;
    nonzero(&id)?;
    Ok(id)
}

fn validate_lifetime(issued: u64, expires: u64, max_lifetime: u64) -> Result<()> {
    let lifetime = expires.checked_sub(issued).ok_or(MediaProtocolError::InvalidLifetime)?;
    if lifetime == 0 || lifetime > max_lifetime {
        return Err(MediaProtocolError::InvalidLifetime);
    }
    expires.checked_add(CLOCK_SKEW).ok_or(MediaProtocolError::TimeOverflow)?;
    Ok(())
}

fn validate_time(issued: u64, expires: u64, max_lifetime: u64, now: u64) -> Result<()> {
    validate_lifetime(issued, expires, max_lifetime)?;
    if issued > now.checked_add(CLOCK_SKEW).ok_or(MediaProtocolError::TimeOverflow)? {
        return Err(MediaProtocolError::Future);
    }
    if now > expires.checked_add(CLOCK_SKEW).ok_or(MediaProtocolError::TimeOverflow)? {
        return Err(MediaProtocolError::Expired);
    }
    Ok(())
}

fn decode<T: Decode>(input: &mut &[u8]) -> Result<T> {
    T::decode(input).map_err(|_| MediaProtocolError::InvalidEncoding)
}

/// Check length before any allocation. Compact<u32> also rejects nonminimal
/// SCALE lengths; fixed fields cannot recurse or allocate.
fn decode_bounded_bytes<'a>(input: &mut &'a [u8], max: usize) -> Result<&'a [u8]> {
    let length = decode::<Compact<u32>>(input)?.0 as usize;
    if length > max {
        return Err(MediaProtocolError::SizeLimit);
    }
    if length > input.len() {
        return Err(MediaProtocolError::InvalidEncoding);
    }
    let (bytes, rest) = input.split_at(length);
    *input = rest;
    Ok(bytes)
}

fn decode_unsigned_fields(input: &mut &[u8]) -> Result<UnsignedAdvertisement> {
    let version = decode(input)?;
    let network = decode(input)?;
    let product_id = std::str::from_utf8(decode_bounded_bytes(input, MAX_PRODUCT_ID_BYTES)?)
        .map_err(|_| MediaProtocolError::InvalidEncoding)?.to_owned();
    Ok(UnsignedAdvertisement {
        version, network, product_id,
        account: decode(input)?,
        endpoint_id: decode(input)?,
        encryption_key: decode(input)?,
        signing_key: decode(input)?,
        issued_at: decode(input)?,
        expires_at: decode(input)?,
    })
}

fn decode_advertisement_fields(input: &mut &[u8]) -> Result<MediaAdvertisement> {
    Ok(MediaAdvertisement {
        fields: decode_unsigned_fields(input)?,
        signature: decode(input)?,
    })
}

fn decode_packet(bytes: &[u8]) -> Result<SealedPacket<'_>> {
    if bytes.len() > MAX_PACKET_BYTES {
        return Err(MediaProtocolError::SizeLimit);
    }
    let mut input = bytes;
    let header = PacketHeader {
        version: decode(&mut input)?,
        sender: decode_advertisement_fields(&mut input)?,
        recipient_account: decode(&mut input)?,
        recipient_endpoint: decode(&mut input)?,
        message_id: decode(&mut input)?,
        issued_at: decode(&mut input)?,
        expires_at: decode(&mut input)?,
    };
    let encapsulated_key = decode(&mut input)?;
    let ciphertext = decode_bounded_bytes(&mut input, MAX_PLAINTEXT_BYTES + TAG_BYTES)?;
    if ciphertext.len() < TAG_BYTES {
        return Err(MediaProtocolError::InvalidEncoding);
    }
    let signature = decode(&mut input)?;
    if !input.is_empty() {
        return Err(MediaProtocolError::InvalidEncoding);
    }
    Ok(SealedPacket { header, encapsulated_key, ciphertext, signature })
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ReplayKey {
    account: [u8; 32],
    endpoint: [u8; 32],
    message: [u8; 32],
}

/// Private to the runtime secrets so replay history cannot accidentally be
/// replaced while continuing to accept ciphertext addressed to the same keys.
struct ReplayWindow {
    capacity: usize,
    entries: BTreeMap<ReplayKey, u64>,
    last_now: u64,
}

impl ReplayWindow {
    fn admit(&mut self, key: ReplayKey, expires: u64, now: u64) -> Result<()> {
        if now < self.last_now {
            return Err(MediaProtocolError::ClockRegression);
        }
        let retain_through = expires.checked_add(CLOCK_SKEW).ok_or(MediaProtocolError::TimeOverflow)?;
        if now > retain_through {
            return Err(MediaProtocolError::Expired);
        }
        self.last_now = now;
        self.entries.retain(|_, expiry| *expiry >= now);
        if self.entries.contains_key(&key) {
            return Err(MediaProtocolError::Replay);
        }
        if self.entries.len() >= self.capacity {
            return Err(MediaProtocolError::ReplayFull);
        }
        self.entries.insert(key, retain_through);
        Ok(())
    }
}

/// UTC anchored to a caller-supplied monotonic clock. Wall-clock rollback never
/// extends expiry; forward corrections become the new anchor. Monotonic values
/// are elapsed durations from a stable caller-owned origin, not wall timestamps.
#[derive(Debug)]
pub(crate) struct EffectiveClock {
    anchor_wall: u64,
    anchor_monotonic: Duration,
    last_monotonic: Duration,
}

impl EffectiveClock {
    pub(crate) fn new(wall: u64, monotonic: Duration) -> Self {
        Self { anchor_wall: wall, anchor_monotonic: monotonic, last_monotonic: monotonic }
    }

    pub(crate) fn now(&mut self, wall: u64, monotonic: Duration) -> Result<u64> {
        if monotonic < self.last_monotonic {
            return Err(MediaProtocolError::ClockRegression);
        }
        let elapsed = monotonic.checked_sub(self.anchor_monotonic)
            .ok_or(MediaProtocolError::ClockRegression)?;
        let advanced = self.anchor_wall.checked_add(elapsed.as_secs())
            .ok_or(MediaProtocolError::TimeOverflow)?;
        self.last_monotonic = monotonic;
        if wall > advanced {
            self.anchor_wall = wall;
            self.anchor_monotonic = monotonic;
            Ok(wall)
        } else {
            Ok(advanced)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn account(seed: u8) -> schnorrkel::Keypair {
        MiniSecretKey::from_bytes(&[seed; 32]).unwrap().expand_to_keypair(ExpansionMode::Ed25519)
    }

    fn fixture(seed: u8, capacity: usize) -> (RuntimeSecrets, VerifiedAdvertisement) {
        let account = account(seed);
        let identity = MediaIdentity {
            network: [0x11; 32],
            product_id: "vox.dot".into(),
            account: account.public.to_bytes(),
        };
        let (encryption_secret, encryption_public) = MediaKem::derive_keypair(&[seed; 32]);
        let keys = RuntimeSecrets {
            identity,
            endpoint_id: [seed; 32],
            encryption_secret,
            encryption_key: encryption_public.to_bytes().into(),
            signing: MiniSecretKey::from_bytes(&[seed.wrapping_add(100); 32]).unwrap()
                .expand_to_keypair(ExpansionMode::Ed25519),
            replay: ReplayWindow { capacity, entries: BTreeMap::new(), last_now: 0 },
        };
        let unsigned = keys.unsigned_advertisement(NOW, NOW + 600).unwrap();
        let signature = account.sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input()).to_bytes();
        let advertisement = unsigned.authenticate(signature, NOW).unwrap();
        (keys, advertisement)
    }

    fn resign(packet: &mut SealedPacket<'_>, keys: &RuntimeSecrets) {
        packet.signature = keys.signing.sign_simple(PACKET_SIGNING_CONTEXT, &packet.signing_input()).to_bytes();
    }

    #[test]
    fn authority_certification_rejects_unsigned_identity_and_lifetime_substitution() {
        let (keys, advertisement) = fixture(1, 2);
        let fields = &advertisement.0.fields;
        let decoded = decode_unsigned_advertisement(&fields.encode(), &keys.identity, NOW).unwrap();
        let signature = account(1).sign_simple(ACCOUNT_SIGNING_CONTEXT, &decoded.account_signing_input()).to_bytes();
        let certified = decoded.authenticate(signature, NOW).unwrap();
        assert_eq!(certified.identity(), keys.identity);
        for dimension in 0..3 {
            let mut changed = fields.clone();
            match dimension {
                0 => changed.account = account(2).public.to_bytes(),
                1 => changed.product_id = "other.dot".into(),
                _ => changed.network = [0x22; 32],
            }
            assert_eq!(decode_unsigned_advertisement(&changed.encode(), &keys.identity, NOW).unwrap_err(), MediaProtocolError::WrongIdentity);
        }
        let mut changed = fields.clone();
        changed.expires_at += 1;
        assert_eq!(validate_unsigned_advertisement(&changed, &keys.identity, NOW).unwrap_err(), MediaProtocolError::InvalidLifetime);
        changed.expires_at = NOW + 600;
        changed.encryption_key = [0; 32];
        assert_eq!(validate_unsigned_advertisement(&changed, &keys.identity, NOW).unwrap_err(), MediaProtocolError::InvalidKey);
        let mut trailing = fields.encode();
        trailing.push(0);
        assert_eq!(decode_unsigned_advertisement(&trailing, &keys.identity, NOW).unwrap_err(), MediaProtocolError::InvalidEncoding);
    }

    #[test]
    fn account_certificate_binds_all_identity_dimensions_and_signing_context() {
        let (keys, advertisement) = fixture(1, 2);
        let unsigned = &advertisement.0.fields;
        let signer = account(1);
        let substrate_signature = signer.sign_simple(b"substrate", &unsigned.account_signing_input()).to_bytes();
        assert_eq!(unsigned.clone().authenticate(substrate_signature, NOW).unwrap_err(), MediaProtocolError::InvalidSignature);
        let endpoint_signature = keys.signing.sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input()).to_bytes();
        assert_eq!(unsigned.clone().authenticate(endpoint_signature, NOW).unwrap_err(), MediaProtocolError::InvalidSignature);

        for dimension in 0..3 {
            let mut changed = advertisement.0.clone();
            match dimension {
                0 => changed.fields.account = account(2).public.to_bytes(),
                1 => changed.fields.product_id = "other.dot".into(),
                _ => changed.fields.network = [0x22; 32],
            }
            let expected = changed.fields.identity();
            assert_eq!(decode_advertisement(&changed.encode(), &expected, NOW).unwrap_err(), MediaProtocolError::InvalidSignature);
        }
        let mut wrong_expected = keys.identity.clone();
        wrong_expected.account = account(2).public.to_bytes();
        assert_eq!(decode_advertisement(&advertisement.0.encode(), &wrong_expected, NOW).unwrap_err(), MediaProtocolError::WrongIdentity);
    }

    #[test]
    fn signed_certificates_still_reject_noncanonical_products_and_bad_keys() {
        let (_, advertisement) = fixture(1, 2);
        let signer = account(1);
        for case in 0..5 {
            let mut unsigned = advertisement.0.fields.clone();
            let expected = match case {
                0 => { unsigned.product_id = "Vox.dot".into(); MediaProtocolError::InvalidProduct }
                1 => { unsigned.signing_key = [0; 32]; MediaProtocolError::InvalidKey }
                2 => { unsigned.encryption_key = [0; 32]; MediaProtocolError::InvalidKey }
                3 => {
                    unsigned.encryption_key = [0; 32];
                    unsigned.encryption_key[0] = 1;
                    MediaProtocolError::InvalidKey
                }
                _ => {
                    unsigned.encryption_key = [0; 32];
                    unsigned.encryption_key[31] = 0x80;
                    MediaProtocolError::InvalidKey
                }
            };
            let signature = signer.sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input()).to_bytes();
            assert_eq!(unsigned.authenticate(signature, NOW).unwrap_err(), expected);
        }
    }

    #[test]
    fn packet_signatures_and_aead_cover_the_complete_header_and_ciphertext() {
        let (sender, sender_ad) = fixture(1, 2);
        let (recipient, recipient_ad) = fixture(2, 2);
        let wire = seal_packet(&sender, &sender_ad, &recipient_ad, b"private SDP", NOW, NOW + 60).unwrap();
        let opened = open_packet(&recipient, &recipient_ad, &wire, NOW).unwrap();
        assert_eq!(opened.plaintext(), b"private SDP");
        assert_eq!(opened.sender().identity(), sender.identity);

        let mut packet = decode_packet(&wire).unwrap();
        packet.header.message_id[0] ^= 1;
        assert_eq!(open_packet(&recipient, &recipient_ad, &packet.encode(), NOW).unwrap_err(), MediaProtocolError::InvalidSignature);
        // Even a valid endpoint signature cannot transplant an old ciphertext
        // under a newly signed header: the header is also HPKE AAD.
        resign(&mut packet, &sender);
        assert_eq!(open_packet(&recipient, &recipient_ad, &packet.encode(), NOW).unwrap_err(), MediaProtocolError::Decryption);

        let mut packet = decode_packet(&wire).unwrap();
        let mut ciphertext = packet.ciphertext.to_vec();
        ciphertext[0] ^= 1;
        packet.ciphertext = &ciphertext;
        assert_eq!(open_packet(&recipient, &recipient_ad, &packet.encode(), NOW).unwrap_err(), MediaProtocolError::InvalidSignature);
        resign(&mut packet, &sender);
        assert_eq!(open_packet(&recipient, &recipient_ad, &packet.encode(), NOW).unwrap_err(), MediaProtocolError::Decryption);
    }

    #[test]
    fn packets_cannot_cross_product_network_account_or_endpoint_routes() {
        let (sender, sender_ad) = fixture(1, 2);
        let (recipient, recipient_ad) = fixture(2, 2);
        let wire = seal_packet(&sender, &sender_ad, &recipient_ad, b"offer", NOW, NOW + 60).unwrap();
        for dimension in 0..4 {
            let mut packet = decode_packet(&wire).unwrap();
            let expected = match dimension {
                0 => { packet.header.sender.fields.product_id = "other.dot".into(); MediaProtocolError::WrongIdentity }
                1 => { packet.header.sender.fields.network = [0x22; 32]; MediaProtocolError::WrongIdentity }
                2 => { packet.header.recipient_account = sender.identity.account; MediaProtocolError::WrongIdentity }
                _ => { packet.header.recipient_endpoint = sender.endpoint_id; MediaProtocolError::WrongEndpoint }
            };
            resign(&mut packet, &sender);
            assert_eq!(open_packet(&recipient, &recipient_ad, &packet.encode(), NOW).unwrap_err(), expected);
        }
        let mut other_product = recipient_ad.0.fields.clone();
        other_product.product_id = "other.dot".into();
        let signature = account(2).sign_simple(ACCOUNT_SIGNING_CONTEXT, &other_product.account_signing_input()).to_bytes();
        let other_product = other_product.authenticate(signature, NOW).unwrap();
        assert_eq!(seal_packet(&sender, &sender_ad, &other_product, b"offer", NOW, NOW + 60).unwrap_err(), MediaProtocolError::WrongIdentity);
    }

    #[test]
    fn restart_is_fenced_by_both_the_endpoint_and_receiver_secret() {
        let (sender, sender_ad) = fixture(1, 2);
        let (recipient, recipient_ad) = fixture(2, 2);
        let wire = seal_packet(&sender, &sender_ad, &recipient_ad, b"offer", NOW, NOW + 60).unwrap();
        let (mut replacement, _) = fixture(3, 2);
        replacement.identity = recipient.identity.clone();
        let unsigned = replacement.unsigned_advertisement(NOW, NOW + 600).unwrap();
        let signature = account(2).sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input()).to_bytes();
        let replacement_ad = unsigned.authenticate(signature, NOW).unwrap();
        assert_eq!(open_packet(&replacement, &replacement_ad, &wire, NOW).unwrap_err(), MediaProtocolError::WrongEndpoint);
        let old_opened = open_packet(&recipient, &recipient_ad, &wire, NOW).unwrap();
        assert_eq!(replacement.admit_replay(&old_opened, NOW).unwrap_err(), MediaProtocolError::WrongEndpoint);

        // Fault injection only: even an accidentally reused endpoint ID cannot
        // make the replacement's distinct HPKE secret decrypt captured traffic.
        replacement.endpoint_id = recipient.endpoint_id;
        let unsigned = replacement.unsigned_advertisement(NOW, NOW + 600).unwrap();
        let signature = account(2).sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input()).to_bytes();
        let replacement_ad = unsigned.authenticate(signature, NOW).unwrap();
        assert_eq!(open_packet(&replacement, &replacement_ad, &wire, NOW).unwrap_err(), MediaProtocolError::Decryption);
    }

    #[test]
    fn bounded_decoding_rejects_trailing_nonminimal_and_attacker_sized_fields() {
        let (sender, sender_ad) = fixture(1, 2);
        let (recipient, recipient_ad) = fixture(2, 2);
        let mut ad_bytes = sender_ad.0.encode();
        ad_bytes.push(0);
        assert_eq!(decode_advertisement(&ad_bytes, &sender.identity, NOW).unwrap_err(), MediaProtocolError::InvalidEncoding);
        let mut oversized_product = vec![1, 0];
        oversized_product.extend_from_slice(&sender.identity.network);
        Compact(u32::MAX).encode_to(&mut oversized_product);
        assert_eq!(decode_advertisement(&oversized_product, &sender.identity, NOW).unwrap_err(), MediaProtocolError::SizeLimit);
        // Product length 7 must use single-byte compact mode, not two bytes.
        let mut nonminimal = sender_ad.0.encode();
        nonminimal.splice(34..35, [29, 0]);
        assert_eq!(decode_advertisement(&nonminimal, &sender.identity, NOW).unwrap_err(), MediaProtocolError::InvalidEncoding);

        let wire = seal_packet(&sender, &sender_ad, &recipient_ad, b"offer", NOW, NOW + 60).unwrap();
        let mut trailing = wire.clone();
        trailing.push(0);
        assert_eq!(open_packet(&recipient, &recipient_ad, &trailing, NOW).unwrap_err(), MediaProtocolError::InvalidEncoding);
        let packet = decode_packet(&wire).unwrap();
        let mut oversized_ciphertext = (&packet.header, &packet.encapsulated_key).encode();
        Compact(u32::MAX).encode_to(&mut oversized_ciphertext);
        assert_eq!(open_packet(&recipient, &recipient_ad, &oversized_ciphertext, NOW).unwrap_err(), MediaProtocolError::SizeLimit);
        assert_eq!(open_packet(&recipient, &recipient_ad, &vec![0; MAX_PACKET_BYTES + 1], NOW).unwrap_err(), MediaProtocolError::SizeLimit);
        assert_eq!(seal_packet(&sender, &sender_ad, &recipient_ad, &vec![0; MAX_PLAINTEXT_BYTES + 1], NOW, NOW + 60).unwrap_err(), MediaProtocolError::SizeLimit);
    }

    #[test]
    fn signed_time_limits_fail_closed_at_future_expiry_and_overflow_boundaries() {
        let (keys, advertisement) = fixture(1, 2);
        assert_eq!(keys.unsigned_advertisement(NOW, NOW + 601).unwrap_err(), MediaProtocolError::InvalidLifetime);
        assert_eq!(keys.unsigned_advertisement(NOW, NOW).unwrap_err(), MediaProtocolError::InvalidLifetime);
        assert_eq!(keys.unsigned_advertisement(u64::MAX - 60, u64::MAX).unwrap_err(), MediaProtocolError::TimeOverflow);
        assert_eq!(decode_advertisement(&advertisement.0.encode(), &keys.identity, NOW - 31).unwrap_err(), MediaProtocolError::Future);
        assert_eq!(decode_advertisement(&advertisement.0.encode(), &keys.identity, NOW + 631).unwrap_err(), MediaProtocolError::Expired);
        assert_eq!(decode_advertisement(&advertisement.0.encode(), &keys.identity, u64::MAX).unwrap_err(), MediaProtocolError::TimeOverflow);
        let (recipient, recipient_ad) = fixture(2, 2);
        let wire = seal_packet(&keys, &advertisement, &recipient_ad, b"offer", NOW, NOW + 60).unwrap();
        assert_eq!(open_packet(&recipient, &recipient_ad, &wire, NOW - 31).unwrap_err(), MediaProtocolError::Future);
        assert_eq!(open_packet(&recipient, &recipient_ad, &wire, NOW + 91).unwrap_err(), MediaProtocolError::Expired);
        assert_eq!(seal_packet(&keys, &advertisement, &recipient_ad, b"offer", NOW, NOW + 61).unwrap_err(), MediaProtocolError::InvalidLifetime);
        // Both certificates, not only the packet's own lifetime, bound expiry.
        assert_eq!(seal_packet(&keys, &advertisement, &recipient_ad, b"offer", NOW + 590, NOW + 620).unwrap_err(), MediaProtocolError::InvalidLifetime);
    }

    #[test]
    fn replay_capacity_never_evicts_a_live_message_and_skew_is_retained() {
        let (sender, sender_ad) = fixture(1, 2);
        let (mut recipient, recipient_ad) = fixture(2, 1);
        let first = seal_packet(&sender, &sender_ad, &recipient_ad, b"first", NOW, NOW + 10).unwrap();
        let second = seal_packet(&sender, &sender_ad, &recipient_ad, b"second", NOW, NOW + 60).unwrap();
        let first = open_packet(&recipient, &recipient_ad, &first, NOW).unwrap();
        let second = open_packet(&recipient, &recipient_ad, &second, NOW).unwrap();
        recipient.admit_replay(&first, NOW).unwrap();
        assert_eq!(recipient.admit_replay(&first, NOW).unwrap_err(), MediaProtocolError::Replay);
        assert_eq!(recipient.admit_replay(&second, NOW).unwrap_err(), MediaProtocolError::ReplayFull);
        assert_eq!(recipient.admit_replay(&first, NOW + 40).unwrap_err(), MediaProtocolError::Replay);
        assert_eq!(recipient.admit_replay(&second, NOW + 40).unwrap_err(), MediaProtocolError::ReplayFull);
        assert_eq!(recipient.admit_replay(&first, NOW + 41).unwrap_err(), MediaProtocolError::Expired);
        recipient.admit_replay(&second, NOW + 41).unwrap();
        assert_eq!(recipient.admit_replay(&second, NOW + 40).unwrap_err(), MediaProtocolError::ClockRegression);
        assert_eq!(recipient.admit_replay(&second, NOW + 41).unwrap_err(), MediaProtocolError::Replay);
    }

    #[test]
    fn effective_clock_preserves_fractional_progress_and_forward_corrections() {
        let mut clock = EffectiveClock::new(100, Duration::ZERO);
        assert_eq!(clock.now(90, Duration::from_millis(900)).unwrap(), 100);
        assert_eq!(clock.now(80, Duration::from_millis(1100)).unwrap(), 101);
        assert_eq!(clock.now(200, Duration::from_millis(1200)).unwrap(), 200);
        assert_eq!(clock.now(70, Duration::from_millis(2300)).unwrap(), 201);
        assert_eq!(clock.now(500, Duration::from_millis(2200)).unwrap_err(), MediaProtocolError::ClockRegression);
        assert_eq!(clock.now(70, Duration::from_millis(3300)).unwrap(), 202);
        let mut overflow = EffectiveClock::new(u64::MAX - 1, Duration::ZERO);
        assert_eq!(overflow.now(0, Duration::from_secs(2)).unwrap_err(), MediaProtocolError::TimeOverflow);
    }

    #[test]
    fn golden_scale_records_and_domain_separated_topics() {
        let unsigned = UnsignedAdvertisement {
            version: 1,
            network: [0x11; 32],
            product_id: "vox.dot".into(),
            account: [0x22; 32],
            endpoint_id: [0x33; 32],
            encryption_key: [0x44; 32],
            signing_key: [0x55; 32],
            issued_at: 0x0102030405060708,
            expires_at: 0x1112131415161718,
        };
        let unsigned_bytes = hex::decode(concat!(
            "0100",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "1c766f782e646f74",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "3333333333333333333333333333333333333333333333333333333333333333",
            "4444444444444444444444444444444444444444444444444444444444444444",
            "5555555555555555555555555555555555555555555555555555555555555555",
            "08070605040302011817161514131211",
        )).unwrap();
        let mut signing_bytes = b"truapi/media/advertisement/v1".to_vec();
        signing_bytes.extend_from_slice(&unsigned_bytes);
        assert_eq!(unsigned.account_signing_input(), signing_bytes);
        let advertisement = MediaAdvertisement { fields: unsigned, signature: [0x66; 64] };
        let mut advertisement_bytes = unsigned_bytes;
        advertisement_bytes.extend_from_slice(&[0x66; 64]);
        assert_eq!(advertisement.encode(), advertisement_bytes);
        let header = PacketHeader {
            version: 1, sender: advertisement, recipient_account: [0x77; 32],
            recipient_endpoint: [0x88; 32], message_id: [0x99; 32],
            issued_at: 1, expires_at: 2,
        };
        let mut header_bytes = vec![1, 0];
        header_bytes.extend_from_slice(&advertisement_bytes);
        header_bytes.extend_from_slice(&[0x77; 32]);
        header_bytes.extend_from_slice(&[0x88; 32]);
        header_bytes.extend_from_slice(&[0x99; 32]);
        header_bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(header.encode(), header_bytes);
        let mut info = b"truapi/media/hpke/v1".to_vec();
        info.extend_from_slice(&[0x11; 32]);
        info.extend_from_slice(b"\x1cvox.dot");
        for field in [0x22, 0x33, 0x77, 0x88] { info.extend_from_slice(&[field; 32]); }
        assert_eq!(hpke_info(&header), info);
        let packet = SealedPacket { header, encapsulated_key: [0xaa; 32], ciphertext: &[1, 2, 3], signature: [0xbb; 64] };
        let mut packet_bytes = header_bytes;
        packet_bytes.extend_from_slice(&[0xaa; 32]);
        packet_bytes.extend_from_slice(&[12, 1, 2, 3]);
        let mut packet_signing = b"truapi/media/packet/v1".to_vec();
        packet_signing.extend_from_slice(&packet_bytes);
        assert_eq!(packet.signing_input(), packet_signing);
        packet_bytes.extend_from_slice(&[0xbb; 64]);
        assert_eq!(packet.encode(), packet_bytes);

        let identity = MediaIdentity {
            network: [0x11; 32], product_id: "vox.dot".into(),
            account: hex::decode("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d").unwrap().try_into().unwrap(),
        };
        assert_eq!(hex::encode(advertisement_topic(&identity).unwrap()), "c6b3f9e3eca8bd18aebfc43c01113bdbba23e32e026e3c6673c74f3aa60dc2f1");
        assert_eq!(hex::encode(inbox_topic(&identity, &[0x33; 32]).unwrap()), "043744fdaed7c307c5354f1fd99a6ac3e1505cb17391ec7eec205f3f2e4c430a");
    }
}
