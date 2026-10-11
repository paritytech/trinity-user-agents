//! Generic authenticated notifications in a bounded standard Gordian Envelope.
//! The container grammar follows draft-mcnally-envelope-12, section 3; it does
//! not interpret application assertions. This profile permits integer-only dCBOR
//! leaves, at most 128 assertions per node, 4096 CBOR items and depth 16.

use std::collections::BTreeMap;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Maximum UTF-8 JSON header length.
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
/// Maximum opaque byte-leaf witness length.
pub const MAX_CARRIER_BYTES: usize = 240 * 1024;
/// Total container bound, including unrelated application assertions.
pub const MAX_FULL_FRAME_BYTES: usize = 256 * 1024;
/// Maximum signed candidates anywhere in a container.
pub const MAX_NOTIFICATION_CANDIDATES: usize = 32;
/// Maximum signed lifetime in milliseconds.
pub const MAX_TTL_MS: u64 = 86_400_000;
/// Maximum creation time ahead of the local clock.
pub const FUTURE_SKEW_MS: u64 = 60_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const PREDICATE: &str = "truapiNotification";

/// Strict metadata: v1 Ed25519, v2 sr25519. Hex is lowercase without a prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationHeader {
    /// One for Ed25519; two for sr25519 under the Substrate signing context.
    pub v: u64,
    /// ASCII product label matching `[a-z0-9._-]{1,128}`.
    pub product: String,
    /// Actual chain genesis hash.
    pub genesis: String,
    /// Actual source channel.
    pub channel: String,
    /// Ordered, distinct authenticated topic subset, between one and four.
    pub topics: Vec<String>,
    /// Application-generated event identifier.
    pub event_id: String,
    /// Creation time in epoch milliseconds.
    pub created_at: u64,
    /// Exclusive expiry time in epoch milliseconds.
    pub expires_at: u64,
    /// SHA-256 of a byte-leaf witness in the carrier, not application assertions.
    pub ciphertext_digest: String,
    /// Raw public key for the algorithm selected by `v`.
    pub sender_key: String,
    /// Raw signature of the version-domain-separated tuple.
    pub signature: String,
}

/// Verified metadata with proven byte membership, not an enrollment or replay decision.
#[derive(Debug)]
pub struct VerifiedNotification {
    /// Authenticated public metadata.
    pub header: NotificationHeader,
}

fn canonical_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_header(header: &NotificationHeader) -> Result<(), String> {
    if !matches!(header.v, 1 | 2)
        || header.product.is_empty()
        || header.product.len() > 128
        || !header.product.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte))
        || !canonical_hex(&header.genesis, 32)
        || !canonical_hex(&header.channel, 32)
        || !canonical_hex(&header.event_id, 32)
        || !canonical_hex(&header.ciphertext_digest, 32)
        || !canonical_hex(&header.sender_key, 32)
        || !canonical_hex(&header.signature, 64)
        || !(1..=4).contains(&header.topics.len())
        || header.topics.iter().enumerate().any(|(index, topic)| !canonical_hex(topic, 32) || header.topics[..index].contains(topic))
        || header.created_at > MAX_SAFE_INTEGER
        || header.expires_at > MAX_SAFE_INTEGER
        || header.expires_at <= header.created_at
        || header.expires_at - header.created_at > MAX_TTL_MS
    {
        return Err("invalid notification header".into());
    }
    Ok(())
}

/// Exact UTF-8 JSON signing tuple. The signature field is not included.
/// sr25519 signs these bytes with schnorrkel's `substrate` context.
pub fn signing_bytes(header: &NotificationHeader) -> Result<Vec<u8>, String> {
    let domain = match header.v {
        1 => "truapi:notification:v1",
        2 => "truapi:notification:v2",
        _ => return Err("unsupported notification version".into()),
    };
    serde_json::to_vec(&(
        domain, header.v, &header.product, &header.genesis,
        &header.channel, &header.topics, &header.event_id, header.created_at,
        header.expires_at, &header.ciphertext_digest, &header.sender_key,
    )).map_err(|error| error.to_string())
}

/// Validate an algorithm-qualified sender policy without interpreting a raw key
/// as authority for a different algorithm.
pub fn valid_sender_policy(sender: &str) -> bool {
    canonical_hex(sender.strip_prefix("sr25519:").unwrap_or(sender), 32)
}

/// Match only the signature algorithm selected by the authenticated version.
pub fn sender_policy_matches(header: &NotificationHeader, sender: &str) -> bool {
    match header.v {
        1 => sender == header.sender_key,
        2 => sender.strip_prefix("sr25519:") == Some(header.sender_key.as_str()),
        _ => false,
    }
}

/// Encode a minimal standard Gordian carrier with a byte-leaf witness.
/// The supplied proof is not verified; callers sign `signing_bytes` first.
pub fn encode_notification_envelope(header: &NotificationHeader, carrier: &[u8]) -> Result<Vec<u8>, String> {
    validate_header(header)?;
    if carrier.len() > MAX_CARRIER_BYTES { return Err("notification carrier too large".into()); }
    if hex::encode(Sha256::digest(carrier)) != header.ciphertext_digest {
        return Err("notification byte witness digest mismatch".into());
    }
    let json = serde_json::to_vec(header).map_err(|error| error.to_string())?;
    if json.len() > MAX_HEADER_BYTES { return Err("notification header too large".into()); }
    let mut frame = Vec::with_capacity(carrier.len() + json.len() + PREDICATE.len() + 32);
    frame.extend_from_slice(&[0xd8, 200, 0x82, 0xd8, 201]);
    append_cbor_head(&mut frame, 2, carrier.len());
    frame.extend_from_slice(carrier);
    frame.extend_from_slice(&[0xa1, 0xd8, 201]);
    append_cbor_head(&mut frame, 3, PREDICATE.len());
    frame.extend_from_slice(PREDICATE.as_bytes());
    frame.extend_from_slice(&[0xd8, 201]);
    append_cbor_head(&mut frame, 3, json.len());
    frame.extend_from_slice(&json);
    if frame.len() > MAX_FULL_FRAME_BYTES { return Err("notification container too large".into()); }
    Ok(frame)
}

fn append_cbor_head(output: &mut Vec<u8>, major: u8, length: usize) {
    if length < 24 {
        output.push((major << 5) | length as u8);
    } else if length < 256 {
        output.extend_from_slice(&[(major << 5) | 24, length as u8]);
    } else if length < 65_536 {
        output.push((major << 5) | 25);
        output.extend_from_slice(&(length as u16).to_be_bytes());
    } else {
        output.push((major << 5) | 26);
        output.extend_from_slice(&(length as u32).to_be_bytes());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind { Leaf, Node, Assertion, Elided, Wrapped }

struct Envelope<'a> {
    kind: Kind,
    digest: [u8; 32],
    bytes: Option<&'a [u8]>,
    text: Option<&'a str>,
    header: Option<&'a str>,
}

struct Cbor<'a> {
    input: &'a [u8],
    offset: usize,
    items_left: usize,
    headers: Vec<&'a str>,
    subjects: BTreeMap<[u8; 32], &'a [u8]>,
}

impl<'a> Cbor<'a> {
    fn head(&mut self, depth: usize) -> Result<(u8, u64), String> {
        if depth > 16 || self.items_left == 0 || self.offset >= self.input.len() {
            return Err("invalid CBOR bounds".into());
        }
        self.items_left -= 1;
        let initial = self.input[self.offset];
        self.offset += 1;
        let major = initial >> 5;
        let additional = initial & 31;
        if additional >= 28 || (major == 7 && additional > 23) {
            return Err("unsupported CBOR value".into());
        }
        let mut argument = u64::from(additional);
        if additional >= 24 {
            let size = 1usize << (additional - 24);
            if size > self.input.len() - self.offset { return Err("truncated CBOR argument".into()); }
            argument = 0;
            for byte in &self.input[self.offset..self.offset + size] {
                argument = argument * 256 + u64::from(*byte);
            }
            self.offset += size;
            if argument > MAX_SAFE_INTEGER || argument < [24, 256, 65_536, 4_294_967_296][usize::from(additional - 24)] {
                return Err("noncanonical CBOR integer".into());
            }
        }
        Ok((major, argument))
    }

    fn data(&mut self, length: u64) -> Result<&'a [u8], String> {
        if length > (self.input.len() - self.offset) as u64 { return Err("truncated CBOR bytes".into()); }
        let start = self.offset;
        self.offset += length as usize;
        Ok(&self.input[start..self.offset])
    }

    fn text(bytes: &'a [u8]) -> Result<&'a str, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "invalid CBOR UTF-8")?;
        if !text.nfc().eq(text.chars()) { return Err("noncanonical CBOR text".into()); }
        Ok(text)
    }

    fn skip(&mut self, depth: usize) -> Result<(), String> {
        let (major, argument) = self.head(depth)?;
        match major {
            0 | 1 => {},
            2 => { self.data(argument)?; },
            3 => { Self::text(self.data(argument)?)?; },
            4 | 5 => {
                let count = argument.checked_mul(if major == 5 { 2 } else { 1 }).ok_or("CBOR item limit")?;
                if count > self.items_left as u64 { return Err("CBOR item limit".into()); }
                let mut prior_key: Option<&[u8]> = None;
                for index in 0..count {
                    let start = self.offset;
                    self.skip(depth + 1)?;
                    if major == 5 && index % 2 == 0 {
                        let key = &self.input[start..self.offset];
                        if prior_key.is_some_and(|prior| prior >= key) { return Err("noncanonical CBOR map".into()); }
                        prior_key = Some(key);
                    }
                }
            },
            6 => self.skip(depth + 1)?,
            7 if matches!(argument, 20..=22) => {},
            _ => return Err("unsupported CBOR simple value".into()),
        }
        Ok(())
    }

    fn envelope(&mut self, depth: usize) -> Result<Envelope<'a>, String> {
        let (major, argument) = self.head(depth)?;
        let mut result = Envelope { kind: Kind::Leaf, digest: [0; 32], bytes: None, text: None, header: None };
        match (major, argument) {
            (6, 201) => {
                let start = self.offset;
                // Inspect simple leaf values without building a CBOR value tree.
                let saved_items = self.items_left;
                let (leaf_major, length) = self.head(depth + 1)?;
                match leaf_major {
                    2 => result.bytes = Some(self.data(length)?),
                    3 => result.text = Some(Self::text(self.data(length)?)?),
                    _ => {
                        self.offset = start;
                        self.items_left = saved_items;
                        self.skip(depth + 1)?;
                    },
                }
                result.digest = Sha256::digest(&self.input[start..self.offset]).into();
                if let Some(bytes) = result.bytes {
                    let digest: [u8; 32] = Sha256::digest(bytes).into();
                    if self.subjects.get(&digest).is_some_and(|prior| *prior != bytes) {
                        return Err("contradictory byte witness".into());
                    }
                    self.subjects.insert(digest, bytes);
                }
            },
            (2, 32) => {
                result.kind = Kind::Elided;
                result.digest.copy_from_slice(self.data(32)?);
            },
            (5, 1) => {
                let predicate = self.envelope(depth + 1)?;
                let object = self.envelope(depth + 1)?;
                result.kind = Kind::Assertion;
                let mut hash = Sha256::new();
                hash.update(predicate.digest);
                hash.update(object.digest);
                result.digest = hash.finalize().into();
                if predicate.text == Some(PREDICATE) {
                    if predicate.kind != Kind::Leaf || object.kind != Kind::Leaf || object.text.is_none() {
                        return Err("ambiguous notification assertion".into());
                    }
                    result.header = object.text;
                    if self.headers.len() >= MAX_NOTIFICATION_CANDIDATES { return Err("notification candidate limit".into()); }
                    self.headers.push(object.text.ok_or("invalid notification header object")?);
                }
            },
            (6, 200) => {
                result.kind = Kind::Wrapped;
                result.digest = Sha256::digest(self.envelope(depth + 1)?.digest).into();
            },
            (4, 2..=129) => {
                let subject = self.envelope(depth + 1)?;
                let mut hash = Sha256::new();
                hash.update(subject.digest);
                result.kind = Kind::Node;
                result.text = subject.text;
                let mut prior: Option<[u8; 32]> = None;
                for _ in 1..argument {
                    let assertion = self.envelope(depth + 1)?;
                    if !matches!(assertion.kind, Kind::Assertion | Kind::Elided) || prior.is_some_and(|digest| digest >= assertion.digest) {
                        return Err("invalid Envelope assertion order or structure".into());
                    }
                    if let Some(header) = assertion.header {
                        if result.header.is_some() { return Err("duplicate notification assertion".into()); }
                        result.header = Some(header);
                    }
                    prior = Some(assertion.digest);
                    hash.update(assertion.digest);
                }
                result.digest = hash.finalize().into();
            },
            _ => return Err("unsupported Envelope structure".into()),
        }
        Ok(result)
    }
}

fn extract(frame: &[u8]) -> Result<Cbor<'_>, String> {
    if frame.len() > MAX_FULL_FRAME_BYTES { return Err("notification container too large".into()); }
    let mut reader = Cbor { input: frame, offset: 0, items_left: 4096, headers: Vec::new(), subjects: BTreeMap::new() };
    if reader.head(0)? != (6, 200) { return Err("not a Gordian Envelope".into()); }
    reader.envelope(1)?;
    if reader.offset != frame.len() { return Err("trailing notification container data".into()); }
    Ok(reader)
}


/// Verify all eligible candidates with a byte-leaf membership witness in the carrier.
/// Malformed containers fail as a whole. Invalid candidate headers/proofs are omitted.
/// Callers separately enforce product authority, approved senders, mute and replay policy.
pub fn verify_frames(
    frame: &[u8], actual_genesis: &str, actual_channel: &str,
    actual_topics: &[String], now_ms: u64,
) -> Result<Vec<VerifiedNotification>, String> {
    if now_ms > MAX_SAFE_INTEGER { return Err("invalid notification clock".into()); }
    let candidates = extract(frame)?;
    Ok(candidates.headers.iter().filter_map(|json| {
        verify_candidate(json, &candidates.subjects, actual_genesis, actual_channel, actual_topics, now_ms).ok()
    }).collect())
}

fn verify_candidate(
    json: &str, subjects: &BTreeMap<[u8; 32], &[u8]>, actual_genesis: &str, actual_channel: &str,
    actual_topics: &[String], now_ms: u64,
) -> Result<VerifiedNotification, String> {
    if json.len() > MAX_HEADER_BYTES { return Err("notification header too large".into()); }
    let header: NotificationHeader = serde_json::from_str(json).map_err(|error| format!("invalid notification JSON: {error}"))?;
    validate_header(&header)?;
    let mut digest = [0u8; 32];
    hex::decode_to_slice(&header.ciphertext_digest, &mut digest).map_err(|error| error.to_string())?;
    let carrier = *subjects.get(&digest).ok_or("missing notification byte witness")?;
    if carrier.len() > MAX_CARRIER_BYTES { return Err("notification carrier too large".into()); }
    if now_ms > MAX_SAFE_INTEGER || header.created_at > now_ms.saturating_add(FUTURE_SKEW_MS) || header.expires_at <= now_ms {
        return Err("notification outside validity window".into());
    }
    if header.genesis != actual_genesis || header.channel != actual_channel
        || !(1..=4).contains(&actual_topics.len())
        || actual_topics.iter().enumerate().any(|(index, topic)| !canonical_hex(topic, 32) || actual_topics[..index].contains(topic))
        || header.topics.iter().any(|topic| !actual_topics.contains(topic)) {
        return Err("notification source mismatch".into());
    }
    let mut key_bytes = [0u8; 32];
    let mut signature_bytes = [0u8; 64];
    hex::decode_to_slice(&header.sender_key, &mut key_bytes).map_err(|error| error.to_string())?;
    hex::decode_to_slice(&header.signature, &mut signature_bytes).map_err(|error| error.to_string())?;
    if header.v == 2 {
        let key = schnorrkel::PublicKey::from_bytes(&key_bytes).map_err(|error| error.to_string())?;
        // Match the JS verifier's rejection of the Ristretto identity key.
        if key_bytes == [0; 32] { return Err("invalid sr25519 public key".into()); }
        let signature = schnorrkel::Signature::from_bytes(&signature_bytes).map_err(|error| error.to_string())?;
        key.verify_simple(b"substrate", &signing_bytes(&header)?, &signature)
            .map_err(|_| "invalid notification signature".to_owned())?;
        return Ok(VerifiedNotification { header });
    }
    let mut field_modulus = [0xff; 32];
    field_modulus[0] = 0xed;
    field_modulus[31] = 0x7f;
    for compressed in [&key_bytes[..], &signature_bytes[..32]] {
        let mut coordinate = [0u8; 32];
        coordinate.copy_from_slice(compressed);
        coordinate[31] &= 0x7f;
        if coordinate.iter().rev().cmp(field_modulus.iter().rev()) != std::cmp::Ordering::Less { return Err("noncanonical Ed25519 point".into()); }
    }
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|error| error.to_string())?;
    key.verify_strict(&signing_bytes(&header)?, &Signature::from_bytes(&signature_bytes)).map_err(|_| "invalid notification signature".to_owned())?;
    Ok(VerifiedNotification { header })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_frame(
        frame: &[u8], actual_genesis: &str, actual_channel: &str,
        actual_topics: &[String], now_ms: u64,
    ) -> Result<VerifiedNotification, String> {
        let candidates = extract(frame)?;
        if candidates.headers.len() != 1 { return Err("expected exactly one notification candidate".into()); }
        verify_candidate(candidates.headers[0], &candidates.subjects, actual_genesis, actual_channel, actual_topics, now_ms)
    }

    // Independent OpenSSL vector, raw seed 00..1f and byte witness "abc".
    const VECTOR: &str = r#"{"v":1,"product":"example.paseo","genesis":"1111111111111111111111111111111111111111111111111111111111111111","channel":"5555555555555555555555555555555555555555555555555555555555555555","topics":["2222222222222222222222222222222222222222222222222222222222222222","3333333333333333333333333333333333333333333333333333333333333333"],"eventId":"4444444444444444444444444444444444444444444444444444444444444444","createdAt":1700000000000,"expiresAt":1700000060000,"ciphertextDigest":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","senderKey":"03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8","signature":"6bc192199982a1b8e850b66b1f0cd85035354e0b788edecddfad7505da94c7347447b5fa4c9a64647045eea6d52e4aecec14dcb16ea64de317b1b3e76b0f830b"}"#;
    const SIGNED: &str = r#"["truapi:notification:v1",1,"example.paseo","1111111111111111111111111111111111111111111111111111111111111111","5555555555555555555555555555555555555555555555555555555555555555",["2222222222222222222222222222222222222222222222222222222222222222","3333333333333333333333333333333333333333333333333333333333333333"],"4444444444444444444444444444444444444444444444444444444444444444",1700000000000,1700000060000,"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8"]"#;
    const NOW: u64 = 1_700_000_000_000;

    struct Element { bytes: Vec<u8>, digest: [u8; 32] }

    fn head(major: u8, length: usize) -> Vec<u8> {
        if length < 24 { return vec![(major << 5) | length as u8]; }
        if length < 256 { return vec![(major << 5) | 24, length as u8]; }
        if length < 65_536 {
            let mut bytes = vec![(major << 5) | 25];
            bytes.extend_from_slice(&(length as u16).to_be_bytes());
            return bytes;
        }
        let mut bytes = vec![(major << 5) | 26];
        bytes.extend_from_slice(&(length as u32).to_be_bytes());
        bytes
    }

    fn leaf(major: u8, value: &[u8]) -> Element {
        let mut raw = head(major, value.len());
        raw.extend_from_slice(value);
        let digest = Sha256::digest(&raw).into();
        let mut bytes = vec![0xd8, 201];
        bytes.extend(raw);
        Element { bytes, digest }
    }

    fn assertion(predicate: &str, object: Element) -> Element {
        let key = leaf(3, predicate.as_bytes());
        let mut bytes = vec![0xa1];
        bytes.extend(key.bytes);
        bytes.extend(object.bytes);
        let mut hash = Sha256::new();
        hash.update(key.digest);
        hash.update(object.digest);
        Element { bytes, digest: hash.finalize().into() }
    }

    fn node(subject: Element, mut assertions: Vec<Element>) -> Element {
        assertions.sort_by_key(|entry| entry.digest);
        let mut bytes = head(4, assertions.len() + 1);
        bytes.extend(subject.bytes);
        let mut hash = Sha256::new();
        hash.update(subject.digest);
        for entry in assertions {
            bytes.extend(entry.bytes);
            hash.update(entry.digest);
        }
        Element { bytes, digest: hash.finalize().into() }
    }

    fn tagged(element: Element) -> Vec<u8> {
        let mut bytes = vec![0xd8, 200];
        bytes.extend(element.bytes);
        bytes
    }

    fn frame(json: &str, body: &[u8]) -> Vec<u8> {
        tagged(node(leaf(2, body), vec![assertion(PREDICATE, leaf(3, json.as_bytes()))]))
    }

    fn verify(bytes: &[u8]) -> Result<VerifiedNotification, String> {
        verify_frame(bytes, &"11".repeat(32), &"55".repeat(32), &["22".repeat(32), "33".repeat(32)], NOW)
    }

    // @scure/sr25519: seed 00..1f, explicit nonce entropy [42;32]; verified with 2.4.0.
    // Shared with JS; verification never relies on a mocked crypto result.
    fn sr25519_vector() -> NotificationHeader {
        let mut header: NotificationHeader = serde_json::from_str(VECTOR).unwrap();
        header.v = 2;
        header.sender_key = "e2111779981618705ecacea1af6ff9350bce2b2dccd03e0c3e01eb0c823d2666".into();
        header.signature = "626d34d4b5ac0674fd7f59d4a84304ff4fab63c1d3bf0907e86be668f5d2624b73bfb67bafe4a3c7c2e3b1d7305a99aecb492183b143666045948f80fae7a784".into();
        header
    }

    #[test]
    fn sr25519_cross_language_profile_and_encoder() {
        let header = sr25519_vector();
        let bytes = encode_notification_envelope(&header, b"abc").unwrap();
        assert_eq!(bytes, frame(&serde_json::to_string(&header).unwrap(), b"abc"));
        assert_eq!(verify(&bytes).unwrap().header.signature, header.signature);
        let expected = SIGNED.replace("truapi:notification:v1\",1", "truapi:notification:v2\",2")
            .replace("03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8", &header.sender_key);
        assert_eq!(signing_bytes(&header).unwrap(), expected.as_bytes());
        assert!(sender_policy_matches(&header, &format!("sr25519:{}", header.sender_key)));
        assert!(!sender_policy_matches(&header, &header.sender_key));
        let v1: NotificationHeader = serde_json::from_str(VECTOR).unwrap();
        assert!(sender_policy_matches(&v1, &v1.sender_key));
        assert!(!sender_policy_matches(&v1, &format!("sr25519:{}", v1.sender_key)));
        assert!(encode_notification_envelope(&header, b"wrong").is_err());
    }

    #[test]
    fn sr25519_rejects_downgrades_wrong_context_and_tampering() {
        let header = sr25519_vector();
        for (field, value) in [
            ("v", serde_json::json!(1)), ("v", serde_json::json!(3)),
            ("product", serde_json::json!("other.paseo")),
            ("eventId", serde_json::json!("66".repeat(32))),
            ("senderKey", serde_json::json!("00".repeat(32))),
            ("signature", serde_json::json!("00".repeat(64))),
            ("ciphertextDigest", serde_json::json!("00".repeat(32))),
        ] {
            let mut changed = serde_json::to_value(&header).unwrap();
            changed[field] = value;
            assert!(verify(&frame(&changed.to_string(), b"abc")).is_err(), "{field}");
        }
        let bytes = encode_notification_envelope(&header, b"abc").unwrap();
        assert!(verify_frame(&bytes, &"00".repeat(32), &header.channel, &header.topics, NOW).is_err());
        assert!(verify_frame(&bytes, &header.genesis, &"00".repeat(32), &header.topics, NOW).is_err());
        assert!(verify_frame(&bytes, &header.genesis, &header.channel, &header.topics[..1], NOW).is_err());
        assert!(verify_frame(&bytes, &header.genesis, &header.channel, &header.topics, header.expires_at).is_err());
        let key = schnorrkel::MiniSecretKey::from_bytes(&[7; 32]).unwrap()
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        let mut signed = header;
        signed.sender_key = hex::encode(key.public.to_bytes());
        for context in [b"substrate".as_slice(), b"not-substrate".as_slice()] {
            signed.signature = hex::encode(key.sign_simple(context, &signing_bytes(&signed).unwrap()).to_bytes());
            assert_eq!(verify(&encode_notification_envelope(&signed, b"abc").unwrap()).is_ok(), context == b"substrate");
        }
        let mut v1: NotificationHeader = serde_json::from_str(VECTOR).unwrap();
        v1.v = 2;
        assert!(verify(&encode_notification_envelope(&v1, b"abc").unwrap()).is_err());
        for invalid in ["ed25519:".to_owned() + &"11".repeat(32), "sr25519:".to_owned() + &"AA".repeat(32), "sr25519:00".into()] {
            assert!(!valid_sender_policy(&invalid));
        }
    }

    #[test]
    fn independent_cross_language_vector() {
        let bytes = frame(VECTOR, b"abc");
        let verified = verify(&bytes).unwrap();
        assert_eq!(signing_bytes(&verified.header).unwrap(), SIGNED.as_bytes());
        let actual_topics = ["66".repeat(32), "33".repeat(32), "22".repeat(32)];
        assert!(verify_frame(&frame(VECTOR, b"abc"), &"11".repeat(32), &"55".repeat(32), &actual_topics, NOW).is_ok());
    }

    #[test]
    fn resolves_generic_byte_membership_and_independent_siblings() {
        let prior = node(leaf(3, b"unrelated subject"), vec![
            assertion("opaque-slot", leaf(2, b"abc")),
            assertion(PREDICATE, leaf(3, VECTOR.as_bytes())),
        ]);
        let forged_json = VECTOR.replace("6bc19219", "00000000");
        let forged = node(leaf(3, b"another subject"), vec![assertion(PREDICATE, leaf(3, forged_json.as_bytes()))]);
        let bytes = tagged(node(leaf(3, b"control"), vec![assertion("first", prior), assertion("second", forged)]));
        let verified = verify_frames(&bytes, &"11".repeat(32), &"55".repeat(32), &["22".repeat(32), "33".repeat(32)], NOW).unwrap();
        assert_eq!(verified.len(), 1);
        assert_eq!(signing_bytes(&verified[0].header).unwrap(), SIGNED.as_bytes());
        assert!(verify(&frame(VECTOR, b"wrong")).is_err());
    }

    #[test]
    fn rejects_ambiguous_json_and_schema() {
        for json in [
            VECTOR.replace("\"v\":1", "\"v\":1,\"v\":1"),
            VECTOR.replace("\"v\":1", "\"v\":1,\"\\u0076\":1"),
            VECTOR.replace("\"v\":1", "\"v\":1,\"extra\":false"),
            VECTOR.replace("\"v\":1,", ""),
            VECTOR.replace("\"v\":1", "\"v\":1.0"),
            VECTOR.replace("\"v\":1", "\"v\":1e0"),
            VECTOR.replace("\"v\":1", "\"v\":-0"),
            format!("{VECTOR} trailing"),
        ] { assert!(verify(&frame(&json, b"abc")).is_err(), "{json}"); }
    }

    #[test]
    fn rejects_invalid_metadata_proofs_and_sources() {
        for (field, value) in [
            ("product", serde_json::json!("😀")), ("product", serde_json::json!("A")),
            ("product", serde_json::json!("x".repeat(129))), ("product", serde_json::json!("")),
            ("genesis", serde_json::json!("AA".repeat(32))), ("channel", serde_json::json!("00".repeat(32))),
            ("eventId", serde_json::json!("66".repeat(32))), ("topics", serde_json::json!([])),
            ("topics", serde_json::json!(vec!["22".repeat(32); 5])),
            ("topics", serde_json::json!(vec!["22".repeat(32); 2])),
            ("createdAt", serde_json::json!(MAX_SAFE_INTEGER + 1)), ("createdAt", serde_json::json!(-1)),
            ("expiresAt", serde_json::json!(NOW)), ("expiresAt", serde_json::json!(NOW + MAX_TTL_MS + 1)),
            ("signature", serde_json::json!("00".repeat(64))), ("senderKey", serde_json::json!("00".repeat(32))),
            ("ciphertextDigest", serde_json::json!("00".repeat(32))),
        ] {
            let mut header: serde_json::Value = serde_json::from_str(VECTOR).unwrap();
            header[field] = value;
            assert!(verify(&frame(&header.to_string(), b"abc")).is_err(), "{field}");
        }
        let bytes = frame(VECTOR, b"abc");
        let topics = ["22".repeat(32), "33".repeat(32)];
        assert!(verify_frame(&bytes, &"00".repeat(32), &"55".repeat(32), &topics, NOW).is_err());
        assert!(verify_frame(&bytes, &"11".repeat(32), &"00".repeat(32), &topics, NOW).is_err());
        assert!(verify_frame(&bytes, &"11".repeat(32), &"55".repeat(32), &topics[..1], NOW).is_err());
        assert!(verify_frame(&bytes, &"11".repeat(32), &"55".repeat(32), &topics, NOW + 60_000).is_err());
        assert!(verify_frame(&bytes, &"11".repeat(32), &"55".repeat(32), &topics, NOW - FUTURE_SKEW_MS - 1).is_err());
    }

    #[test]
    fn rejects_container_ambiguities_and_resource_exhaustion() {
        assert!(verify(&[]).is_err());
        assert!(verify(&vec![0; MAX_FULL_FRAME_BYTES + 1]).is_err());
        assert!(verify(&frame(&" ".repeat(MAX_HEADER_BYTES + 1), b"abc")).is_err());
        assert!(verify(&frame(VECTOR, &vec![0; MAX_CARRIER_BYTES + 1])).is_err());
        let mut trailing = frame(VECTOR, b"abc"); trailing.push(0);
        assert!(verify(&trailing).is_err());
        let canonical = frame(VECTOR, b"abc");
        let mut noncanonical = vec![0xd9, 0, 200]; noncanonical.extend_from_slice(&canonical[2..]);
        assert!(verify(&noncanonical).is_err());
        let duplicate = tagged(node(leaf(2, b"abc"), vec![
            assertion(PREDICATE, leaf(3, VECTOR.as_bytes())), assertion(PREDICATE, leaf(3, b"different")),
        ]));
        assert!(verify(&duplicate).is_err());
        let candidates = (0..33).map(|index| assertion(&format!("entry-{index}"), node(leaf(3, b"subject"), vec![assertion(PREDICATE, leaf(3, VECTOR.as_bytes()))]))).collect();
        assert!(verify(&tagged(node(leaf(2, b"abc"), candidates))).is_err());
        let mut nested = leaf(2, b"abc");
        for _ in 0..20 { nested = node(leaf(3, b"subject"), vec![assertion("nested", nested)]); }
        assert!(verify(&tagged(nested)).is_err());
    }
}

#[cfg(test)]
mod strict_equation_tests {
    use super::*;

    #[test]
    fn rejects_cofactor_only_proof_but_accepts_exact_mixed_key_proof() {
        // Independent scalar a=r=1 fixtures with order-two T=(0,-1).
        let mut header = NotificationHeader {
            v: 1, product: "example.paseo".into(), genesis: "11".repeat(32),
            channel: "55".repeat(32), topics: vec!["22".repeat(32), "33".repeat(32)],
            event_id: "44".repeat(32), created_at: 1_700_000_000_000, expires_at: 1_700_000_060_000,
            ciphertext_digest: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            sender_key: "5866666666666666666666666666666666666666666666666666666666666666".into(),
            signature: "95999999999999999999999999999999999999999999999999999999999999994d0d311b42ae0fbd5231e2b7e106d734ca5e5045ba0882ac54c3f232e5718300".into(),
        };
        let mut subjects = BTreeMap::new();
        subjects.insert(Sha256::digest(b"abc").into(), &b"abc"[..]);
        assert!(verify_candidate(&serde_json::to_string(&header).unwrap(), &subjects,
            &header.genesis, &header.channel, &header.topics, header.created_at).is_err());
        header.event_id = format!("{}04", "00".repeat(31));
        header.sender_key = "9599999999999999999999999999999999999999999999999999999999999999".into();
        header.signature = "58666666666666666666666666666666666666666666666666666666666666663114ba2ce5c96de9e15fbc99e889f60672480566a4420d0d7806399239ed5a06".into();
        assert!(verify_candidate(&serde_json::to_string(&header).unwrap(), &subjects,
            &header.genesis, &header.channel, &header.topics, header.created_at).is_ok());
    }
}
