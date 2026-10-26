//! Encryption to an X25519 public key, shared by SSO pairing and NFT cheques.
//!
//! A key agreement gives a shared secret, HKDF-SHA256 (no salt, no info) turns
//! it into a ChaCha20-Poly1305 key, and every ciphertext carries its random
//! 12-byte nonce in front: `nonce ‖ ciphertext ‖ tag`.

use chacha20poly1305::{
	ChaCha20Poly1305,
	aead::{Aead, KeyInit},
};
use hkdf::Hkdf;
use sha2::Sha256;
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};

/// Byte length of the ChaCha20-Poly1305 nonce prepended to encrypted payloads.
pub const AEAD_NONCE_LEN: usize = 12;
const AEAD_TAG_LEN: usize = 16;

/// Why encrypting to, or decrypting from, an X25519 key failed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SealError {
	/// The platform randomness source failed.
	#[error("failed to generate randomness: {0}")]
	Random(#[from] getrandom::Error),
	/// The peer key is a low-order point, so the shared secret would be all zeros.
	#[error("invalid X25519 public key {}: a low-order point", hex::encode(.0))]
	LowOrderKey([u8; 32]),
	/// The encrypted bytes cannot hold a nonce and a tag.
	#[error("encrypted message is too short: {0} bytes, expected at least {min}", min = AEAD_NONCE_LEN + AEAD_TAG_LEN)]
	TooShort(usize),
	/// ChaCha20-Poly1305 refused to encrypt.
	#[error("encryption failed")]
	Encrypt,
	/// The ciphertext does not authenticate under the derived key.
	#[error("decryption failed")]
	Decrypt,
}

/// Bytes encrypted to one X25519 public key under a fresh ephemeral key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedBox {
	/// The ephemeral public key the recipient agrees with.
	pub ephemeral_public: [u8; 32],
	/// `nonce ‖ ciphertext ‖ tag`.
	pub encrypted: Vec<u8>,
}

/// Encrypts `plaintext` so only the holder of `recipient`'s secret can read it.
/// Fails on a low-order `recipient`.
pub fn seal_to(recipient: [u8; 32], plaintext: &[u8]) -> Result<SealedBox, SealError> {
	let (ephemeral_secret, _) = generate_x25519_keypair()?;
	let mut nonce = [0u8; AEAD_NONCE_LEN];
	getrandom::getrandom(&mut nonce)?;
	seal_with(ephemeral_secret, nonce, recipient, plaintext)
}

/// [`seal_to`] with a given ephemeral secret and nonce, so the output is
/// fixed. Both must be fresh for every message.
fn seal_with(
	ephemeral_secret: [u8; 32],
	nonce: [u8; AEAD_NONCE_LEN],
	recipient: [u8; 32],
	plaintext: &[u8],
) -> Result<SealedBox, SealError> {
	let shared = shared_secret(ephemeral_secret, recipient)?;
	let encrypted = encrypt_with_nonce(aead_key(&shared), plaintext, nonce)?;
	Ok(SealedBox { ephemeral_public: x25519_public_key(ephemeral_secret), encrypted })
}

/// Decrypts what [`seal_to`] produced for the public key of `secret`.
pub fn open_sealed(secret: [u8; 32], sealed: &SealedBox) -> Result<Vec<u8>, SealError> {
	let shared = shared_secret(secret, sealed.ephemeral_public)?;
	decrypt(aead_key(&shared), &sealed.encrypted)
}

/// The X25519 shared secret of `own_secret_key` and `peer_public_key`.
/// Refuses a low-order peer key.
pub fn shared_secret(
	own_secret_key: [u8; 32],
	peer_public_key: [u8; 32],
) -> Result<[u8; 32], SealError> {
	let secret = StaticSecret::from(own_secret_key);
	let shared = secret.diffie_hellman(&PublicKey::from(peer_public_key));
	if !shared.was_contributory() {
		return Err(SealError::LowOrderKey(peer_public_key));
	}
	Ok(shared.to_bytes())
}

/// The ChaCha20-Poly1305 key for `shared_secret`.
pub fn aead_key(shared_secret: &[u8; 32]) -> [u8; 32] {
	let hkdf = Hkdf::<Sha256>::new(None, shared_secret);
	let mut key = [0u8; 32];
	hkdf.expand(&[], &mut key)
		.expect("32 bytes is a valid HKDF-SHA256 output length; qed");
	key
}

/// Encrypts `plaintext` under `key` with the given `nonce`.
pub fn encrypt_with_nonce(
	key: [u8; 32],
	plaintext: &[u8],
	nonce: [u8; AEAD_NONCE_LEN],
) -> Result<Vec<u8>, SealError> {
	let ciphertext = ChaCha20Poly1305::new((&key).into())
		.encrypt((&nonce).into(), plaintext)
		.map_err(|_| SealError::Encrypt)?;
	Ok([nonce.as_slice(), &ciphertext].concat())
}

/// Decrypts `nonce ‖ ciphertext ‖ tag` under `key`.
pub fn decrypt(key: [u8; 32], encrypted: &[u8]) -> Result<Vec<u8>, SealError> {
	let Some((nonce, ciphertext)) = encrypted.split_first_chunk::<AEAD_NONCE_LEN>() else {
		return Err(SealError::TooShort(encrypted.len()));
	};
	if ciphertext.len() < AEAD_TAG_LEN {
		return Err(SealError::TooShort(encrypted.len()));
	}
	ChaCha20Poly1305::new((&key).into())
		.decrypt(nonce.into(), ciphertext)
		.map_err(|_| SealError::Decrypt)
}

/// A random X25519 keypair as `(secret, public)`.
pub fn generate_x25519_keypair() -> Result<([u8; 32], [u8; 32]), getrandom::Error> {
	let mut secret = [0u8; 32];
	getrandom::getrandom(&mut secret)?;
	Ok((secret, x25519_public_key(secret)))
}

/// X25519 public key matching a raw 32-byte secret scalar.
pub fn x25519_public_key(secret_bytes: [u8; 32]) -> [u8; 32] {
	PublicKey::from(&StaticSecret::from(secret_bytes)).to_bytes()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The recipient of a sealed box reads it back, and a flipped bit or the
	/// wrong secret is refused, so a cheque cannot be altered or opened by
	/// anyone else.
	#[test]
	fn a_sealed_box_opens_only_for_its_recipient_and_unaltered() {
		let (secret, public) = generate_x25519_keypair().unwrap();
		let (other_secret, _) = generate_x25519_keypair().unwrap();
		let mut sealed = seal_to(public, b"item secrets").unwrap();

		assert_eq!(open_sealed(secret, &sealed).unwrap(), b"item secrets");
		assert_eq!(open_sealed(other_secret, &sealed), Err(SealError::Decrypt));
		let last = sealed.encrypted.len() - 1;
		sealed.encrypted[last] ^= 1;
		assert_eq!(open_sealed(secret, &sealed), Err(SealError::Decrypt));
	}

	/// Other hosts open what this host seals, so a fixed ephemeral key, nonce,
	/// recipient and plaintext must always give the same bytes.
	#[test]
	fn a_fixed_ephemeral_key_and_nonce_seal_to_fixed_bytes() {
		let recipient = x25519_public_key([3; 32]);

		let sealed = seal_with([1; 32], [2; AEAD_NONCE_LEN], recipient, b"cheque").unwrap();

		assert_eq!(sealed.ephemeral_public, x25519_public_key([1; 32]));
		assert_eq!(
			hex::encode(&sealed.encrypted),
			"0202020202020202020202025c53e60d79544edad7cc6d7092017a87fb2c13d52476"
		);
	}

	/// An all-zero shared secret would let anyone decrypt, so a low-order key
	/// is refused before anything is encrypted.
	#[test]
	fn sealing_to_a_low_order_key_is_refused() {
		assert_eq!(seal_to([0; 32], b"secret"), Err(SealError::LowOrderKey([0; 32])));
	}

	/// Bytes that cannot hold a nonce and a tag are reported as too short
	/// rather than as a failed decryption.
	#[test]
	fn encrypted_bytes_shorter_than_nonce_and_tag_are_too_short() {
		let short = [0; AEAD_NONCE_LEN + AEAD_TAG_LEN - 1];

		assert_eq!(decrypt([1; 32], &short), Err(SealError::TooShort(short.len())));
	}
}
