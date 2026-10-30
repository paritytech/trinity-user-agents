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
use truapi::latest::X25519PublicKey;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

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
	#[error("invalid X25519 public key {}: a low-order point", hex::encode(.0.0))]
	LowOrderKey(X25519PublicKey),
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

/// An X25519 secret scalar, wiped from memory when dropped.
#[derive(Clone)]
pub struct X25519PrivateKey(Zeroizing<[u8; 32]>);

impl X25519PrivateKey {
	/// A fresh random secret.
	pub fn generate() -> Result<Self, getrandom::Error> {
		let mut secret = Zeroizing::new([0u8; 32]);
		getrandom::getrandom(secret.as_mut())?;
		Ok(Self(secret))
	}

	/// The public key that matches this secret.
	pub fn public_key(&self) -> X25519PublicKey {
		X25519PublicKey(PublicKey::from(&StaticSecret::from(*self.0)).to_bytes())
	}

	/// The raw secret, for storing it.
	pub fn as_bytes(&self) -> &[u8; 32] {
		&self.0
	}
}

impl From<[u8; 32]> for X25519PrivateKey {
	fn from(secret: [u8; 32]) -> Self {
		Self(Zeroizing::new(secret))
	}
}

/// A secret with its public key, which always matches it.
pub struct X25519Keypair {
	private_key: X25519PrivateKey,
	public_key: X25519PublicKey,
}

impl X25519Keypair {
	/// A fresh random keypair.
	pub fn generate() -> Result<Self, getrandom::Error> {
		X25519PrivateKey::generate().map(Self::from)
	}

	/// The secret half.
	pub fn private_key(&self) -> &X25519PrivateKey {
		&self.private_key
	}

	/// The public half.
	pub fn public_key(&self) -> X25519PublicKey {
		self.public_key
	}
}

impl From<X25519PrivateKey> for X25519Keypair {
	fn from(private_key: X25519PrivateKey) -> Self {
		let public_key = private_key.public_key();
		Self { private_key, public_key }
	}
}

/// What an X25519 key agreement gives both sides, wiped from memory when dropped.
pub struct X25519SharedSecret(Zeroizing<[u8; 32]>);

impl X25519SharedSecret {
	/// The secret `own` shares with the holder of `peer`. Refuses a low-order
	/// `peer`, whose shared secret anyone could compute.
	pub fn from_ecdh(own: &X25519PrivateKey, peer: X25519PublicKey) -> Result<Self, SealError> {
		let shared = StaticSecret::from(*own.0).diffie_hellman(&PublicKey::from(peer.0));
		if !shared.was_contributory() {
			return Err(SealError::LowOrderKey(peer));
		}
		Ok(Self(Zeroizing::new(shared.to_bytes())))
	}

	/// The ChaCha20-Poly1305 key both sides derive from this secret.
	pub fn aead_key(&self) -> AeadKey {
		let hkdf = Hkdf::<Sha256>::new(None, self.0.as_slice());
		let mut key = Zeroizing::new([0u8; 32]);
		hkdf.expand(&[], key.as_mut())
			.expect("32 bytes is a valid HKDF-SHA256 output length; qed");
		AeadKey(key)
	}

	/// The raw secret, for protocols that derive more from it than one key.
	pub fn as_bytes(&self) -> &[u8; 32] {
		&self.0
	}
}

/// A ChaCha20-Poly1305 key, wiped from memory when dropped.
pub struct AeadKey(Zeroizing<[u8; 32]>);

impl AeadKey {
	/// Encrypts `plaintext` with the given `nonce` into `nonce ‖ ciphertext ‖ tag`.
	pub fn encrypt_with_nonce(
		&self,
		plaintext: &[u8],
		nonce: [u8; AEAD_NONCE_LEN],
	) -> Result<Vec<u8>, SealError> {
		let ciphertext = self
			.cipher()
			.encrypt((&nonce).into(), plaintext)
			.map_err(|_| SealError::Encrypt)?;
		Ok([nonce.as_slice(), &ciphertext].concat())
	}

	/// Decrypts `nonce ‖ ciphertext ‖ tag`.
	pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, SealError> {
		let Some((nonce, ciphertext)) = encrypted.split_first_chunk::<AEAD_NONCE_LEN>() else {
			return Err(SealError::TooShort(encrypted.len()));
		};
		if ciphertext.len() < AEAD_TAG_LEN {
			return Err(SealError::TooShort(encrypted.len()));
		}
		self.cipher().decrypt(nonce.into(), ciphertext).map_err(|_| SealError::Decrypt)
	}

	fn cipher(&self) -> ChaCha20Poly1305 {
		ChaCha20Poly1305::new(self.0.as_ref().into())
	}
}

/// Bytes encrypted to one X25519 public key under a fresh ephemeral key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedBox {
	/// The ephemeral public key the recipient agrees with.
	pub ephemeral_public: X25519PublicKey,
	/// `nonce ‖ ciphertext ‖ tag`.
	pub encrypted: Vec<u8>,
}

impl SealedBox {
	/// Encrypts `plaintext` so only the holder of `recipient`'s secret can
	/// read it. Fails on a low-order `recipient`.
	pub fn seal(recipient: X25519PublicKey, plaintext: &[u8]) -> Result<Self, SealError> {
		let ephemeral = X25519PrivateKey::generate()?;
		let mut nonce = [0u8; AEAD_NONCE_LEN];
		getrandom::getrandom(&mut nonce)?;
		Self::seal_with(&ephemeral, nonce, recipient, plaintext)
	}

	/// Decrypts the box with the secret of the key it was sealed to.
	pub fn open(&self, recipient: &X25519PrivateKey) -> Result<Vec<u8>, SealError> {
		let shared = X25519SharedSecret::from_ecdh(recipient, self.ephemeral_public)?;
		shared.aead_key().decrypt(&self.encrypted)
	}

	/// [`Self::seal`] with a given ephemeral key and nonce, so the output is
	/// fixed. Both must be fresh for every message.
	fn seal_with(
		ephemeral: &X25519PrivateKey,
		nonce: [u8; AEAD_NONCE_LEN],
		recipient: X25519PublicKey,
		plaintext: &[u8],
	) -> Result<Self, SealError> {
		let shared = X25519SharedSecret::from_ecdh(ephemeral, recipient)?;
		let encrypted = shared.aead_key().encrypt_with_nonce(plaintext, nonce)?;
		Ok(Self { ephemeral_public: ephemeral.public_key(), encrypted })
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The recipient of a sealed box reads it back, and a flipped bit or the
	/// wrong secret is refused, so a cheque cannot be altered or opened by
	/// anyone else.
	#[test]
	fn a_sealed_box_opens_only_for_its_recipient_and_unaltered() {
		let recipient = X25519Keypair::generate().unwrap();
		let other = X25519PrivateKey::generate().unwrap();
		let mut sealed = SealedBox::seal(recipient.public_key(), b"item secrets").unwrap();

		assert_eq!(sealed.open(recipient.private_key()).unwrap(), b"item secrets");
		assert_eq!(sealed.open(&other), Err(SealError::Decrypt));
		let last = sealed.encrypted.len() - 1;
		sealed.encrypted[last] ^= 1;
		assert_eq!(sealed.open(recipient.private_key()), Err(SealError::Decrypt));
	}

	/// Other hosts open what this host seals, so a fixed ephemeral key, nonce,
	/// recipient and plaintext must always give the same bytes.
	#[test]
	fn a_fixed_ephemeral_key_and_nonce_seal_to_fixed_bytes() {
		let ephemeral = X25519PrivateKey::from([1; 32]);
		let recipient = X25519PrivateKey::from([3; 32]).public_key();

		let sealed =
			SealedBox::seal_with(&ephemeral, [2; AEAD_NONCE_LEN], recipient, b"cheque").unwrap();

		assert_eq!(sealed.ephemeral_public, ephemeral.public_key());
		assert_eq!(
			hex::encode(&sealed.encrypted),
			"0202020202020202020202025c53e60d79544edad7cc6d7092017a87fb2c13d52476"
		);
	}

	/// An all-zero shared secret would let anyone decrypt, so a low-order key
	/// is refused before anything is encrypted.
	#[test]
	fn sealing_to_a_low_order_key_is_refused() {
		let low_order = X25519PublicKey([0; 32]);

		assert_eq!(SealedBox::seal(low_order, b"secret"), Err(SealError::LowOrderKey(low_order)));
	}

	/// Bytes that cannot hold a nonce and a tag are reported as too short
	/// rather than as a failed decryption.
	#[test]
	fn encrypted_bytes_shorter_than_nonce_and_tag_are_too_short() {
		let short = [0; AEAD_NONCE_LEN + AEAD_TAG_LEN - 1];
		let key = AeadKey(Zeroizing::new([1; 32]));

		assert_eq!(key.decrypt(&short), Err(SealError::TooShort(short.len())));
	}
}
