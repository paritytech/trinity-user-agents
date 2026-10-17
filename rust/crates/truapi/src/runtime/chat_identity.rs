//! Native identity-route primitives for the non-exportable Chat crypto boundary.
//!
//! Every secret is an explicit Host-private input. Only the actor may open a
//! verified external incoming route; these primitives are not product methods.

use chacha20poly1305::aead::{Aead, AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatIdentityError {
    InvalidPeerKey,
    InvalidCiphertext,
    Unavailable,
}

pub(crate) fn chat_shared_secret(
    private_key: &[u8; 32],
    peer_public_key: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, ChatIdentityError> {
    if !is_canonical_x25519_public_key(peer_public_key) {
        return Err(ChatIdentityError::InvalidPeerKey);
    }
    let shared = Zeroizing::new(
        StaticSecret::from(*private_key)
            .diffie_hellman(&PublicKey::from(*peer_public_key))
            .to_bytes(),
    );
    if *shared == [0; 32] {
        return Err(ChatIdentityError::InvalidPeerKey);
    }
    Ok(shared)
}

fn is_canonical_x25519_public_key(key: &[u8; 32]) -> bool {
    const FIELD_MODULUS: [u8; 32] = [
        0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];
    key.iter().rev().cmp(FIELD_MODULUS.iter().rev()).is_lt()
}

fn identity_proof_hash(
    shared_secret: &[u8; 32],
    identity: &[u8; 32],
    device: &[u8; 32],
) -> blake2b_simd::Hash {
    const CONTEXT: &[u8] = b"mds-chat-request";
    let mut payload = [0; 65 + CONTEXT.len()];
    payload[..32].copy_from_slice(identity);
    payload[32..64].copy_from_slice(device);
    payload[64] = (CONTEXT.len() as u8) << 2;
    payload[65..].copy_from_slice(CONTEXT);
    blake2b_simd::Params::new()
        .hash_length(32)
        .key(shared_secret)
        .hash(&payload)
}

pub(crate) fn chat_device_identity_proof(
    shared_secret: &[u8; 32],
    identity: &[u8; 32],
    device: &[u8; 32],
) -> [u8; 32] {
    let mut proof = [0; 32];
    proof.copy_from_slice(identity_proof_hash(shared_secret, identity, device).as_bytes());
    proof
}

pub(crate) fn verify_chat_device_identity_proof(
    shared_secret: &[u8; 32],
    identity: &[u8; 32],
    device: &[u8; 32],
    proof: &[u8; 32],
) -> bool {
    // blake2b_simd::Hash compares equal-length slices in constant time.
    identity_proof_hash(shared_secret, identity, device).eq(proof.as_slice())
}

pub(crate) fn chat_identity_session_id(
    shared_secret: &[u8; 32],
    first_account_id: &[u8; 32],
    second_account_id: &[u8; 32],
) -> [u8; 32] {
    chat_route_id(
        shared_secret,
        b"session",
        first_account_id,
        second_account_id,
    )
}

pub(crate) fn chat_request_channel_id(
    shared_secret: &[u8; 32],
    requester_account_id: &[u8; 32],
    acceptor_account_id: &[u8; 32],
) -> [u8; 32] {
    chat_route_id(
        shared_secret,
        b"chat-request",
        requester_account_id,
        acceptor_account_id,
    )
}

fn chat_route_id(
    shared_secret: &[u8; 32],
    domain: &[u8],
    first: &[u8; 32],
    second: &[u8; 32],
) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .key(shared_secret)
        .to_state()
        .update(domain)
        .update(first)
        .update(second)
        .update(b"//")
        .finalize();
    let mut output = [0; 32];
    output.copy_from_slice(hash.as_bytes());
    output
}

fn native_root_key(shared_secret: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(&[]), shared_secret)
        .expand(&[], &mut *key)
        .expect("32-byte key fits HKDF-SHA256 output limit");
    key
}

pub(crate) fn native_root_seal(
    shared_secret: &[u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>, ChatIdentityError> {
    let key = native_root_key(shared_secret);
    let mut nonce = [0; 12];
    getrandom::getrandom(&mut nonce).map_err(|_| ChatIdentityError::Unavailable)?;
    let mut combined = Zeroizing::new(Vec::with_capacity(nonce.len() + plaintext.len() + 16));
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(plaintext);
    let tag = ChaCha20Poly1305::new((&*key).into())
        .encrypt_in_place_detached(Nonce::from_slice(&nonce), &[], &mut combined[12..])
        .map_err(|_| ChatIdentityError::Unavailable)?;
    combined.extend_from_slice(&tag);
    Ok(std::mem::take(&mut *combined))
}

pub(crate) fn native_root_open(
    shared_secret: &[u8; 32],
    combined: &[u8],
) -> Result<Zeroizing<Vec<u8>>, ChatIdentityError> {
    if combined.len() < 28 {
        return Err(ChatIdentityError::InvalidCiphertext);
    }
    let key = native_root_key(shared_secret);
    ChaCha20Poly1305::new((&*key).into())
        .decrypt(Nonce::from_slice(&combined[..12]), &combined[12..])
        .map(Zeroizing::new)
        .map_err(|_| ChatIdentityError::InvalidCiphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex32(value: &str) -> [u8; 32] {
        hex::decode(value).unwrap().try_into().unwrap()
    }

    #[test]
    fn identity_proof_and_routes_match_native_chat_vectors() {
        let peer = PublicKey::from(&StaticSecret::from([0x22; 32])).to_bytes();
        let shared = chat_shared_secret(&[0x11; 32], &peer).unwrap();
        assert_eq!(
            chat_device_identity_proof(&shared, &[0x33; 32], &[0x44; 32]),
            hex32("0263d1995da865e34e06de38b4f4c0c88524e2e591b1ae6714578219bffad333")
        );
        assert_eq!(
            chat_identity_session_id(&shared, &[0x33; 32], &[0x55; 32]),
            hex32("460db8611d842e65414f9eea4aa74d3fe1ac2e31468d4fbebededd914be28422")
        );
        assert_eq!(
            chat_identity_session_id(&shared, &[0x55; 32], &[0x33; 32]),
            hex32("bfb5eb8c0b959f95b3ab09bd0f8001ab80f100cf5bb617640534372ab777c5c3")
        );
        assert_eq!(
            chat_request_channel_id(&shared, &[0x33; 32], &[0x55; 32]),
            hex32("576f71aa7f51aa340f411c20779c35f476361d8008247db367a8ce4d7e087d70")
        );
        assert_eq!(
            chat_request_channel_id(&shared, &[0x55; 32], &[0x33; 32]),
            hex32("19de8cf16554a8463d0f8af7ad23717f4106463af331ee33f297b7367c8fe9fa")
        );
    }

    #[test]
    fn peer_binding_verifies_reciprocally_and_rejects_substitution() {
        let sender = PublicKey::from(&StaticSecret::from([0x11; 32])).to_bytes();
        let shared = chat_shared_secret(&[0x22; 32], &sender).unwrap();
        let proof = hex32("0263d1995da865e34e06de38b4f4c0c88524e2e591b1ae6714578219bffad333");
        assert!(verify_chat_device_identity_proof(
            &shared,
            &[0x33; 32],
            &[0x44; 32],
            &proof
        ));
        assert!(!verify_chat_device_identity_proof(
            &shared,
            &[0x34; 32],
            &[0x44; 32],
            &proof
        ));
        assert!(!verify_chat_device_identity_proof(
            &shared,
            &[0x33; 32],
            &[0x45; 32],
            &proof
        ));
        let mut corrupted = proof;
        corrupted[31] ^= 1;
        assert!(!verify_chat_device_identity_proof(
            &shared,
            &[0x33; 32],
            &[0x44; 32],
            &corrupted
        ));
    }

    #[test]
    fn native_root_cipher_authenticates_peer_ciphertext_and_nonce() {
        let sender = PublicKey::from(&StaticSecret::from([0x11; 32])).to_bytes();
        let recipient = PublicKey::from(&StaticSecret::from([0x22; 32])).to_bytes();
        let send_secret = chat_shared_secret(&[0x11; 32], &recipient).unwrap();
        let receive_secret = chat_shared_secret(&[0x22; 32], &sender).unwrap();
        let ciphertext = native_root_seal(&send_secret, b"native request").unwrap();
        assert_eq!(
            native_root_open(&receive_secret, &ciphertext)
                .unwrap()
                .as_slice(),
            b"native request"
        );
        let other_secret = chat_shared_secret(&[0x44; 32], &sender).unwrap();
        assert_eq!(
            native_root_open(&other_secret, &ciphertext),
            Err(ChatIdentityError::InvalidCiphertext)
        );
        for index in [0, ciphertext.len() - 1] {
            let mut corrupted = ciphertext.clone();
            corrupted[index] ^= 1;
            assert_eq!(
                native_root_open(&receive_secret, &corrupted),
                Err(ChatIdentityError::InvalidCiphertext)
            );
        }
        assert_eq!(
            native_root_open(&receive_secret, &ciphertext[..27]),
            Err(ChatIdentityError::InvalidCiphertext)
        );
    }

    #[test]
    fn shared_secret_rejects_low_order_and_noncanonical_peers() {
        assert_eq!(
            chat_shared_secret(&[0x11; 32], &[0; 32]),
            Err(ChatIdentityError::InvalidPeerKey)
        );
        let mut noncanonical = PublicKey::from(&StaticSecret::from([0x22; 32])).to_bytes();
        noncanonical[31] |= 0x80;
        assert_eq!(
            chat_shared_secret(&[0x11; 32], &noncanonical),
            Err(ChatIdentityError::InvalidPeerKey)
        );
        let mut modulus = [0xff; 32];
        modulus[0] = 0xed;
        modulus[31] = 0x7f;
        assert_eq!(
            chat_shared_secret(&[0x11; 32], &modulus),
            Err(ChatIdentityError::InvalidPeerKey)
        );
    }
}
