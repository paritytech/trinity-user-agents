//! H3 · Cheque: coin secrets sealed to a receivable, in RFC-17's
//! `CoinPaymentCheque`.
//!
//! Sealing uses the chat's primitives: an ephemeral X25519 key agreed with
//! the receivable, a BLAKE2b-256 key over the shared secret, the ephemeral
//! public key and the receivable, and ChaCha20-Poly1305 with
//! `SCALE((version, to, amount))` as associated data. So only the
//! receivable's holder opens a cheque (invariant 1), and neither its
//! receivable nor its amount can be changed (invariant 2). `sealed` is the
//! version byte, the ephemeral public key, the nonce, then the ciphertext.

use chacha20poly1305::{
	ChaCha20Poly1305, Nonce,
	aead::{Aead, KeyInit, Payload},
};
use parity_scale_codec::{Decode, Encode};
use sp_crypto_hashing::blake2_256;
use truapi::latest::CoinPaymentCheque;
use x25519_dalek::{PublicKey, StaticSecret};

use super::receivable::{Receivable, ReceivableSecret};
use crate::coinage::{CoinSecret, Exponent};

/// The sealing layout this release writes and opens.
const VERSION: u8 = 1;

/// The bytes before the ciphertext: version, ephemeral public key, nonce.
const HEADER: usize = 1 + 32 + 12;

/// What a cheque carries inside its seal.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ChequeContents {
	/// The coins, each with its denomination.
	pub coins: Vec<(CoinSecret, Exponent)>,
	/// The payer's return receivable; none on a refund (invariant 5).
	pub return_to: Option<Receivable>,
	/// The claim deadline, in Unix milliseconds of network time
	/// (invariant 6).
	pub claim_by: u64,
}

/// A cheque's identity: BLAKE2b-256 of its SCALE encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct ChequeHash(pub [u8; 32]);

/// Why a cheque was not sealed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum SealError {
	/// The receivable is a low-order point, with which anyone could derive
	/// the key.
	#[display("the receivable is a low-order point")]
	LowOrderReceivable,
	/// The coins do not add up to the amount (invariant 3).
	#[display("the coins do not add up to the amount")]
	AmountMismatch,
}

/// Why a cheque did not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum OpenError {
	/// The key agreement gives an all-zero shared secret.
	#[display("the key agreement is with a low-order point")]
	LowOrderReceivable,
	/// The version byte is not one this release reads.
	#[display("the cheque's sealing version is unknown")]
	UnknownVersion,
	/// It was tampered with, or sealed to another receivable.
	#[display("the cheque does not authenticate")]
	NotAuthentic,
	/// The plaintext is not `SCALE(ChequeContents)`.
	#[display("the cheque's contents do not decode")]
	Undecodable,
}

/// Seals `contents` to `to` for `amount` cents, with the ephemeral secret
/// and nonce the caller drew.
pub fn seal(
	to: Receivable,
	amount: u32,
	contents: &ChequeContents,
	ephemeral: [u8; 32],
	nonce: [u8; 12],
) -> Result<CoinPaymentCheque, SealError> {
	let declared: u64 = contents.coins.iter().map(|(_, exponent)| exponent.value()).sum();
	if declared != u64::from(amount) {
		return Err(SealError::AmountMismatch);
	}
	let ephemeral = StaticSecret::from(ephemeral);
	let ephemeral_public = PublicKey::from(&ephemeral);
	let shared = ephemeral.diffie_hellman(&PublicKey::from(to.0));
	if !shared.was_contributory() {
		return Err(SealError::LowOrderReceivable);
	}
	let cipher = cipher(shared.as_bytes(), &ephemeral_public, to);
	let aad = (VERSION, to.0, amount).encode();
	let plaintext = contents.encode();
	let ciphertext = cipher
		.encrypt(Nonce::from_slice(&nonce), Payload { msg: &plaintext, aad: &aad })
		.expect("ChaCha20-Poly1305 encrypts any message that fits in memory");
	let sealed = [[VERSION].as_slice(), ephemeral_public.as_bytes(), &nonce, &ciphertext].concat();
	Ok(CoinPaymentCheque { id: to.0, amount, encrypted_secrets: sealed })
}

/// The contents of `cheque`, opened with the secret of its receivable. The
/// coins are not compared with the amount here: the depositor does that.
pub fn open(secret: &ReceivableSecret, cheque: &CoinPaymentCheque) -> Result<ChequeContents, OpenError> {
	let sealed = &cheque.encrypted_secrets;
	match sealed.first() {
		Some(&VERSION) => {},
		_ => return Err(OpenError::UnknownVersion),
	}
	if sealed.len() < HEADER {
		return Err(OpenError::NotAuthentic);
	}
	let ephemeral_public: [u8; 32] = sealed[1..33].try_into().expect("the header holds 32 key bytes");
	let ephemeral_public = PublicKey::from(ephemeral_public);
	let shared = secret.static_secret().diffie_hellman(&ephemeral_public);
	if !shared.was_contributory() {
		return Err(OpenError::LowOrderReceivable);
	}
	let to = Receivable(cheque.id);
	let cipher = cipher(shared.as_bytes(), &ephemeral_public, to);
	let aad = (VERSION, cheque.id, cheque.amount).encode();
	let plaintext = cipher
		.decrypt(Nonce::from_slice(&sealed[33..HEADER]), Payload { msg: &sealed[HEADER..], aad: &aad })
		.map_err(|_| OpenError::NotAuthentic)?;
	ChequeContents::decode(&mut plaintext.as_slice()).map_err(|_| OpenError::Undecodable)
}

/// Whether a cheque can be sealed to `to`: it is not a low-order point.
pub fn accepts(to: Receivable) -> bool {
	StaticSecret::from([1; 32]).diffie_hellman(&PublicKey::from(to.0)).was_contributory()
}

/// The cheque's identity.
pub fn cheque_hash(cheque: &CoinPaymentCheque) -> ChequeHash {
	ChequeHash(blake2_256(&cheque.encode()))
}

/// The AEAD keyed with `BLAKE2b-256(shared ‖ ephemeral public key ‖ to)`.
fn cipher(shared: &[u8; 32], ephemeral_public: &PublicKey, to: Receivable) -> ChaCha20Poly1305 {
	let key = blake2_256(&[shared.as_slice(), ephemeral_public.as_bytes(), &to.0].concat());
	ChaCha20Poly1305::new((&key).into())
}

#[cfg(test)]
mod tests;
