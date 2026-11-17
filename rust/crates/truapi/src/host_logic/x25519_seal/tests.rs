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
