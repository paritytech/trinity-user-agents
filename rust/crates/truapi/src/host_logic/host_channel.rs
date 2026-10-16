//! The host channel: a pairwise channel between two hosts, bootstrapped once
//! over Chat and independent of it afterwards (seity `doc/HOST_CHANNEL_PLAN.md`).
//!
//! Each host derives one hard X25519 key per network and peer from its root
//! entropy. The public half travels to the peer once, in a Chat v2 bootstrap
//! frame. From then on both hosts compute the same pair secret, and from it
//! the statement topic they meet on, the key that seals every message, and a
//! statement channel per message kind. Nothing here does I/O: the runtime
//! posts and subscribes, this module only derives, seals and opens.
//!
//! ```text
//! channel  = x25519 from entropy under "host-channel/v1" ‖ genesis ‖ peer identity
//! pair     = PRF(x25519(my secret, peer public), "pair/v1")
//! topic    = PRF(pair, "topic/v1")
//! seal key = PRF(pair, "seal/v1")                ChaCha20-Poly1305, nonce ‖ ciphertext ‖ tag
//! channel  = PRF(pair, "channel/v1" ‖ kind)      one statement per kind, last write wins
//! ```
//!
//! The seal is the authentication: only the two hosts can derive the key, so
//! a statement that does not open is not theirs. Whoever signs the statement
//! on the store is postage, not provenance.
//!
//! Prototype trims, recorded in the plan: a fixed topic per pair (no
//! rotation), and no acknowledgement of the bootstrap.

use parity_scale_codec::{Decode, Encode};
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::host_logic::sso::pairing::derive_x25519_keypair_from_entropy;

/// Domain of the per-peer channel key, completed by the network genesis and
/// the peer's root identity.
const CHANNEL_KEY_DOMAIN: &[u8] = b"host-channel/v1";
const PAIR_LABEL: &[u8] = b"pair/v1";
const TOPIC_LABEL: &[u8] = b"topic/v1";
const SEAL_LABEL: &[u8] = b"seal/v1";
const CHANNEL_LABEL: &[u8] = b"channel/v1";

/// Kind of the first message the channel carries: a profile reference the
/// sender's host discloses to this peer, or its withdrawal.
pub const KIND_PROFILE_REFERENCE: &str = "profile-reference";

/// Longest kind the envelope accepts; a kind is a short label, not data.
pub const MAX_KIND_LEN: usize = 64;

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// Why a channel operation failed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostChannelError {
	/// The peer's public key is not a contributory X25519 point.
	#[error("peer channel key is invalid")]
	InvalidPeerKey,
	/// The kind is empty or longer than [`MAX_KIND_LEN`].
	#[error("message kind is invalid")]
	InvalidKind,
	/// The bytes are not a sealed message from this pair.
	#[error("message does not open")]
	DoesNotOpen,
	/// The system random source failed.
	#[error("random source failed")]
	Random,
}

/// This host's channel key for one peer on one network.
pub struct ChannelKey {
	secret: Zeroizing<[u8; 32]>,
	public: [u8; 32],
}

impl ChannelKey {
	/// Derive the key for `peer_identity` on the network with `genesis`.
	/// Every device holding the same entropy derives the same key.
	pub fn derive(entropy: &[u8], genesis: &[u8; 32], peer_identity: &[u8; 32]) -> Self {
		let mut domain = Vec::with_capacity(CHANNEL_KEY_DOMAIN.len() + 64);
		domain.extend_from_slice(CHANNEL_KEY_DOMAIN);
		domain.extend_from_slice(genesis);
		domain.extend_from_slice(peer_identity);
		let (secret, public) = derive_x25519_keypair_from_entropy(entropy, &domain);
		Self { secret: Zeroizing::new(secret), public }
	}

	/// The half that travels to the peer in the bootstrap frame.
	pub fn public(&self) -> [u8; 32] {
		self.public
	}

	/// The pair with `peer_public`, the key the peer's bootstrap carried.
	pub fn pair(&self, peer_public: &[u8; 32]) -> Result<Pair, HostChannelError> {
		let shared = StaticSecret::from(*self.secret)
			.diffie_hellman(&PublicKey::from(*peer_public))
			.to_bytes();
		if shared == [0; 32] {
			return Err(HostChannelError::InvalidPeerKey);
		}
		Ok(Pair { secret: Zeroizing::new(prf(&shared, PAIR_LABEL)) })
	}
}

/// What both hosts of a pair compute identically.
pub struct Pair {
	secret: Zeroizing<[u8; 32]>,
}

impl core::fmt::Debug for Pair {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Pair").field("secret", &"[REDACTED]").finish()
	}
}

impl core::fmt::Debug for ChannelKey {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("ChannelKey")
			.field("public", &hex::encode(self.public))
			.field("secret", &"[REDACTED]")
			.finish()
	}
}

impl Pair {
	/// The statement topic both hosts post on and watch.
	pub fn topic(&self) -> [u8; 32] {
		prf(&self.secret, TOPIC_LABEL)
	}

	/// The statement channel for `kind`: a newer statement of the same kind
	/// replaces the older one on the store.
	pub fn channel(&self, kind: &str) -> Result<[u8; 32], HostChannelError> {
		check_kind(kind)?;
		let mut label = Vec::with_capacity(CHANNEL_LABEL.len() + kind.len());
		label.extend_from_slice(CHANNEL_LABEL);
		label.extend_from_slice(kind.as_bytes());
		Ok(prf(&self.secret, &label))
	}

	fn seal_key(&self) -> Zeroizing<[u8; 32]> {
		Zeroizing::new(prf(&self.secret, SEAL_LABEL))
	}

	/// Seal `message` with a fresh nonce.
	pub fn seal(&self, message: &Message) -> Result<Vec<u8>, HostChannelError> {
		let mut nonce = [0u8; NONCE_LEN];
		getrandom::getrandom(&mut nonce).map_err(|_| HostChannelError::Random)?;
		self.seal_with_nonce(message, &nonce)
	}

	/// Seal with a caller-chosen nonce. Never reuse a nonce under one pair.
	pub fn seal_with_nonce(
		&self,
		message: &Message,
		nonce: &[u8; NONCE_LEN],
	) -> Result<Vec<u8>, HostChannelError> {
		use chacha20poly1305::aead::{Aead, KeyInit};
		check_kind(&message.kind)?;
		let cipher = chacha20poly1305::ChaCha20Poly1305::new((&*self.seal_key()).into());
		let ciphertext = cipher
			.encrypt(nonce.into(), message.encode().as_slice())
			.map_err(|_| HostChannelError::Random)?;
		let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
		out.extend_from_slice(nonce);
		out.extend_from_slice(&ciphertext);
		Ok(out)
	}

	/// Open a sealed message. Anything the pair did not seal does not open.
	pub fn open(&self, sealed: &[u8]) -> Result<Message, HostChannelError> {
		use chacha20poly1305::aead::{Aead, KeyInit};
		if sealed.len() < NONCE_LEN + TAG_LEN {
			return Err(HostChannelError::DoesNotOpen);
		}
		let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
		let cipher = chacha20poly1305::ChaCha20Poly1305::new((&*self.seal_key()).into());
		let plaintext = cipher
			.decrypt(nonce.into(), ciphertext)
			.map_err(|_| HostChannelError::DoesNotOpen)?;
		let message = Message::decode(&mut plaintext.as_slice())
			.map_err(|_| HostChannelError::DoesNotOpen)?;
		check_kind(&message.kind).map_err(|_| HostChannelError::DoesNotOpen)?;
		Ok(message)
	}
}

/// What travels on the channel: a kind and the bytes that kind defines. A
/// host ignores kinds it does not know. A kind prefixed with a product id and
/// a colon belongs to that product and is carried unopened (not built yet).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Message {
	/// Which payload codec applies.
	pub kind: String,
	/// The payload, in that kind's codec.
	pub payload: Vec<u8>,
}

/// Payload of [`KIND_PROFILE_REFERENCE`]: the same fields the Chat relay's
/// personal frame carries, so the receiving host records it through the same
/// function with the same dedupe.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileReference {
	/// The disclosure revision; a receiver keeps the highest it has seen.
	pub revision: u64,
	/// The product on the sender's side that disclosed the reference.
	pub discloser_product_id: String,
	/// The reference, or `None` to withdraw it.
	pub reference: Option<String>,
}

impl ProfileReference {
	/// Wrap into the envelope.
	pub fn into_message(self) -> Message {
		Message { kind: KIND_PROFILE_REFERENCE.to_owned(), payload: self.encode() }
	}

	/// Unwrap from the envelope; `None` for another kind.
	pub fn from_message(message: &Message) -> Option<Result<Self, HostChannelError>> {
		if message.kind != KIND_PROFILE_REFERENCE {
			return None;
		}
		Some(
			Self::decode(&mut message.payload.as_slice())
				.map_err(|_| HostChannelError::DoesNotOpen),
		)
	}
}

fn check_kind(kind: &str) -> Result<(), HostChannelError> {
	if kind.is_empty() || kind.len() > MAX_KIND_LEN || kind.chars().any(char::is_control) {
		return Err(HostChannelError::InvalidKind);
	}
	Ok(())
}

/// Keyed BLAKE2b-256 as a PRF: `secret` is the key, `label` the message.
fn prf(secret: &[u8; 32], label: &[u8]) -> [u8; 32] {
	blake2b_simd::Params::new()
		.hash_length(32)
		.key(secret)
		.hash(label)
		.as_bytes()
		.try_into()
		.expect("hash_length(32) configures BLAKE2b output to exactly 32 bytes; qed")
}

#[cfg(test)]
mod tests {
	use super::*;

	const ENTROPY_A: [u8; 32] = [0x11; 32];
	const ENTROPY_B: [u8; 32] = [0x22; 32];
	const GENESIS: [u8; 32] = [0x33; 32];
	// The identities are what the other side knows each host as.
	const IDENTITY_A: [u8; 32] = [0xaa; 32];
	const IDENTITY_B: [u8; 32] = [0xbb; 32];

	fn pair_ab() -> (Pair, Pair) {
		let a = ChannelKey::derive(&ENTROPY_A, &GENESIS, &IDENTITY_B);
		let b = ChannelKey::derive(&ENTROPY_B, &GENESIS, &IDENTITY_A);
		(a.pair(&b.public()).unwrap(), b.pair(&a.public()).unwrap())
	}

	#[test]
	fn derivation_is_deterministic_and_scoped() {
		let key = ChannelKey::derive(&ENTROPY_A, &GENESIS, &IDENTITY_B);
		assert_eq!(key.public(), ChannelKey::derive(&ENTROPY_A, &GENESIS, &IDENTITY_B).public());
		assert_ne!(key.public(), ChannelKey::derive(&ENTROPY_A, &GENESIS, &IDENTITY_A).public());
		assert_ne!(key.public(), ChannelKey::derive(&ENTROPY_A, &[0x34; 32], &IDENTITY_B).public());
		assert_ne!(key.public(), ChannelKey::derive(&ENTROPY_B, &GENESIS, &IDENTITY_B).public());
		// Pinned so a change to the derivation is a deliberate one.
		assert_eq!(
			hex::encode(key.public()),
			"564740a61a7747097bfd0e0a526a73a4f5efbbad7a87626e5ac43599c158be4c",
		);
	}

	#[test]
	fn both_sides_meet_on_one_topic_and_key() {
		let (a, b) = pair_ab();
		assert_eq!(a.topic(), b.topic());
		assert_eq!(a.channel("x").unwrap(), b.channel("x").unwrap());
		assert_ne!(a.channel("x").unwrap(), a.channel("y").unwrap());
		assert_ne!(a.topic(), a.channel("x").unwrap());
		// A third party pairs with A to a different place.
		let c = ChannelKey::derive(&[0x55; 32], &GENESIS, &IDENTITY_A);
		let a_for_c = ChannelKey::derive(&ENTROPY_A, &GENESIS, &[0xcc; 32]);
		assert_ne!(a_for_c.pair(&c.public()).unwrap().topic(), a.topic());
	}

	#[test]
	fn a_degenerate_peer_key_is_refused() {
		let a = ChannelKey::derive(&ENTROPY_A, &GENESIS, &IDENTITY_B);
		assert_eq!(a.pair(&[0; 32]).unwrap_err(), HostChannelError::InvalidPeerKey);
	}

	#[test]
	fn sealed_messages_open_only_for_the_pair() {
		let (a, b) = pair_ab();
		let message = ProfileReference {
			revision: 3,
			discloser_product_id: "seity.dot".into(),
			reference: Some("seity-contacts:v1:00".into()),
		}
		.into_message();
		let sealed = a.seal_with_nonce(&message, &[9; 12]).unwrap();
		assert_eq!(sealed.len(), 12 + message.encode().len() + 16);
		assert_eq!(b.open(&sealed).unwrap(), message);
		assert_eq!(
			ProfileReference::from_message(&b.open(&sealed).unwrap())
				.unwrap()
				.unwrap()
				.reference
				.as_deref(),
			Some("seity-contacts:v1:00")
		);
		let mut tampered = sealed.clone();
		tampered[20] ^= 1;
		assert_eq!(b.open(&tampered).unwrap_err(), HostChannelError::DoesNotOpen);
		assert_eq!(b.open(&sealed[..20]).unwrap_err(), HostChannelError::DoesNotOpen);
		let stranger = ChannelKey::derive(&[0x55; 32], &GENESIS, &IDENTITY_A)
			.pair(&ChannelKey::derive(&ENTROPY_A, &GENESIS, &[0xcc; 32]).public())
			.unwrap();
		assert_eq!(stranger.open(&sealed).unwrap_err(), HostChannelError::DoesNotOpen);
	}

	#[test]
	fn unknown_kinds_pass_through_and_bad_kinds_do_not_seal() {
		let (a, b) = pair_ab();
		let other = Message { kind: "cheque".into(), payload: vec![1, 2, 3] };
		let opened = b.open(&a.seal_with_nonce(&other, &[1; 12]).unwrap()).unwrap();
		assert_eq!(opened, other);
		assert!(ProfileReference::from_message(&opened).is_none());
		let withdrawal = ProfileReference {
			revision: 4,
			discloser_product_id: "seity.dot".into(),
			reference: None,
		}
		.into_message();
		assert_eq!(ProfileReference::from_message(&withdrawal).unwrap().unwrap().reference, None);
		for kind in ["", "a\n", &"k".repeat(MAX_KIND_LEN + 1)] {
			let bad = Message { kind: kind.into(), payload: vec![] };
			assert_eq!(
				a.seal_with_nonce(&bad, &[0; 12]).unwrap_err(),
				HostChannelError::InvalidKind
			);
			assert_eq!(a.channel(kind).unwrap_err(), HostChannelError::InvalidKind);
		}
	}
}
