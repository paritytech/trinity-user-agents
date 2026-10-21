//! HOP pool entries of the person chat, byte for byte as the iOS and Android
//! apps write them: a random 32-byte ticket per file, entries sealed with
//! ChaCha20-Poly1305 under a key the ticket derives, identified by the
//! blake2b-256 of the sealed bytes, and claimed or acked with a key the ticket
//! derives too.

use chacha20poly1305::{ChaCha20Poly1305, KeyInit, aead::Aead};
use parity_scale_codec::{Decode, Encode};
use schnorrkel::{ExpansionMode, Keypair, MiniSecretKey};

use crate::host_internal::bulletin::preimage_cid;

/// The largest plaintext an inline entry holds; larger files are chunked.
pub const HOP_INLINE_MAX_BYTES: usize = 1_999_936;

/// The plaintext size of each chunk of a chunked entry.
pub const HOP_CHUNK_BYTES: usize = 2_000_000;

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// The plaintext of a root entry.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HopPoolEntry {
	/// The only version.
	#[codec(index = 0)]
	V1(HopPoolEntryV1),
}

/// A version 1 root entry.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HopPoolEntryV1 {
	/// The whole file.
	#[codec(index = 0)]
	Inline(Vec<u8>),
	/// A file in chunks: the identifiers of the chunk entries, in order.
	#[codec(index = 1)]
	Chunked {
		/// The plaintext size of the file.
		total_size: u64,
		/// The identifier of each chunk entry.
		chunks: Vec<Vec<u8>>,
	},
}

/// The keys a ticket derives.
pub struct TicketKeys {
	/// The ChaCha20-Poly1305 key of every entry of the file.
	pub encryption_key: [u8; 32],
	/// The sr25519 key that claims and acks the entries; its public key is the
	/// recipient of the submit.
	pub signer: Keypair,
}

fn keyed_blake2b(key: &[u8], message: &[u8]) -> [u8; 32] {
	let mut out = [0; 32];
	out.copy_from_slice(
		blake2b_simd::Params::new().hash_length(32).key(key).hash(message).as_bytes(),
	);
	out
}

/// The keys of `ticket`.
pub fn ticket_keys(ticket: &[u8; 32]) -> TicketKeys {
	let seed = keyed_blake2b(ticket, b"signer");
	let signer = MiniSecretKey::from_bytes(&seed)
		.expect("a 32-byte seed is a mini secret key; qed")
		.expand_to_keypair(ExpansionMode::Ed25519);
	TicketKeys { encryption_key: keyed_blake2b(ticket, b"encryption"), signer }
}

/// The identifier of a sealed entry: what is claimed, acked and fetched by CID.
pub fn entry_identifier(entry: &[u8]) -> [u8; 32] {
	sp_crypto_hashing::blake2_256(entry)
}

/// What the uploader signs to submit `entry` at `submit_timestamp_ms`.
pub fn submit_payload(entry: &[u8], submit_timestamp_ms: u64) -> [u8; 32] {
	let mut message = b"hop-submit-v1:".to_vec();
	message.extend_from_slice(&entry_identifier(entry));
	message.extend_from_slice(&submit_timestamp_ms.to_le_bytes());
	sp_crypto_hashing::blake2_256(&message)
}

/// What the ticket key signs to claim the entry `identifier`.
pub fn claim_payload(identifier: &[u8; 32]) -> [u8; 32] {
	let mut message = b"hop-claim-v1:".to_vec();
	message.extend_from_slice(identifier);
	sp_crypto_hashing::blake2_256(&message)
}

/// What the ticket key signs to ack the entry `identifier`.
pub fn ack_payload(identifier: &[u8; 32]) -> [u8; 32] {
	let mut message = b"hop-ack-v1:".to_vec();
	message.extend_from_slice(identifier);
	sp_crypto_hashing::blake2_256(&message)
}

/// The CID under which Bulletin serves an entry promoted on chain.
pub fn bitswap_cid(identifier: &[u8; 32]) -> String {
	preimage_cid(identifier)
}

/// `plaintext` sealed under `key`: a random 12-byte nonce, then the ciphertext and its tag.
pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
	let mut nonce = [0; NONCE_LEN];
	getrandom::getrandom(&mut nonce)
		.map_err(|error| format!("no randomness for a nonce: {error}"))?;
	let mut sealed = nonce.to_vec();
	sealed.extend(
		ChaCha20Poly1305::new(key.into())
			.encrypt((&nonce).into(), plaintext)
			.map_err(|error| format!("sealing failed: {error}"))?,
	);
	Ok(sealed)
}

/// The plaintext of `sealed`, or `None` when it does not open with `key`.
pub fn open(key: &[u8; 32], sealed: &[u8]) -> Option<Vec<u8>> {
	if sealed.len() < NONCE_LEN + TAG_LEN {
		return None;
	}
	let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
	ChaCha20Poly1305::new(key.into()).decrypt(nonce.into(), ciphertext).ok()
}

/// The entries of one file, ready to submit: the chunk entries first, then the root.
pub struct PreparedUpload {
	/// The sealed chunk entries of a chunked file, in order; empty for an inline one.
	pub chunks: Vec<Vec<u8>>,
	/// The sealed root entry, whose identifier names the file.
	pub root: Vec<u8>,
}

impl PreparedUpload {
	/// The identifier of the root entry.
	pub fn identifier(&self) -> [u8; 32] {
		entry_identifier(&self.root)
	}
}

/// Seals `data` as the entries of one file under `ticket`.
pub fn prepare_upload(ticket: &[u8; 32], data: &[u8]) -> Result<PreparedUpload, String> {
	let key = ticket_keys(ticket).encryption_key;
	if data.len() <= HOP_INLINE_MAX_BYTES {
		let root = seal(&key, &HopPoolEntry::V1(HopPoolEntryV1::Inline(data.to_vec())).encode())?;
		return Ok(PreparedUpload { chunks: Vec::new(), root });
	}
	let chunks = data
		.chunks(HOP_CHUNK_BYTES)
		.map(|chunk| seal(&key, chunk))
		.collect::<Result<Vec<_>, _>>()?;
	let root = HopPoolEntry::V1(HopPoolEntryV1::Chunked {
		total_size: data.len() as u64,
		chunks: chunks.iter().map(|chunk| entry_identifier(chunk).to_vec()).collect(),
	});
	Ok(PreparedUpload { root: seal(&key, &root.encode())?, chunks })
}

/// Opens a root entry claimed under `ticket`.
pub fn open_root(ticket: &[u8; 32], sealed: &[u8]) -> Result<HopPoolEntryV1, String> {
	let plaintext = open(&ticket_keys(ticket).encryption_key, sealed)
		.ok_or_else(|| "the entry does not open with its ticket".to_string())?;
	let HopPoolEntry::V1(entry) = HopPoolEntry::decode(&mut plaintext.as_slice())
		.map_err(|error| format!("the entry is not a pool entry: {error}"))?;
	Ok(entry)
}

/// Opens a chunk entry claimed under `ticket`.
pub fn open_chunk(ticket: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>, String> {
	open(&ticket_keys(ticket).encryption_key, sealed)
		.ok_or_else(|| "a chunk does not open with its ticket".to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn range() -> [u8; 32] {
		core::array::from_fn(|index| index as u8)
	}

	// The vectors of the chat product's TypeScript core (`test/vectors.test.ts`),
	// which the native apps' derivations produce too.
	#[test]
	fn derivations_match_the_native_apps() {
		let ticket = range();
		assert_eq!(
			hex::encode(ticket_keys(&ticket).encryption_key),
			"780d195b37b816da293d2296d160a8e0afcb428f8054fa22c01d3960e14c82f1"
		);
		assert_eq!(
			hex::encode(claim_payload(&ticket)),
			"1b17fdf115876627f63ba2d6bf1e3bfc37f3c7bda0256cd99a78e4e4187682d6"
		);
		assert_eq!(
			hex::encode(ack_payload(&ticket)),
			"fae04ade958e93665e466e9144ad211f0e11ca1b6e80b48ba2217c27d8ccbbf7"
		);
		assert_eq!(
			bitswap_cid(&ticket),
			"bafk2bzaceaaacaqdaqcqmbyibefawdanbyhraeiscmkbkfqxdamrugy4dupb6"
		);
		// Envelope of an inline batch, `00 00 20 <32 bytes>`.
		assert_eq!(
			hex::encode(HopPoolEntry::V1(HopPoolEntryV1::Inline(vec![7; 8])).encode()),
			"00002007070707070707 07".replace(' ', "")
		);
	}

	#[test]
	fn a_file_opens_with_its_ticket_inline_or_chunked() {
		let ticket = [9; 32];
		let small = vec![1; 1_000];
		let inline = prepare_upload(&ticket, &small).unwrap();
		assert!(inline.chunks.is_empty());
		assert_eq!(open_root(&ticket, &inline.root), Ok(HopPoolEntryV1::Inline(small)));

		let large: Vec<u8> = (0..HOP_CHUNK_BYTES * 2 + 5).map(|index| index as u8).collect();
		let chunked = prepare_upload(&ticket, &large).unwrap();
		assert_eq!(chunked.chunks.len(), 3);
		let HopPoolEntryV1::Chunked { total_size, chunks } =
			open_root(&ticket, &chunked.root).unwrap()
		else {
			panic!("a file over the inline limit is chunked");
		};
		assert_eq!(total_size, large.len() as u64);
		let reassembled: Vec<u8> = chunked
			.chunks
			.iter()
			.zip(&chunks)
			.flat_map(|(entry, identifier)| {
				assert_eq!(&entry_identifier(entry).to_vec(), identifier);
				open_chunk(&ticket, entry).unwrap()
			})
			.collect();
		assert_eq!(reassembled, large);
		assert!(open_root(&[8; 32], &inline.root).is_err());
	}
}
