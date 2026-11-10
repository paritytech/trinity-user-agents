//! The CIDs Bulletin serves preimages under.

use core::str::FromStr;

use super::preimage_key;

/// CIDv1, the `raw` codec, then the multihash header: blake2b-256 (0xb220 as a
/// varint) of 32 bytes.
const PREIMAGE_CID_PREFIX: [u8; 6] = [0x01, 0x55, 0xa0, 0xe4, 0x02, 0x20];
/// Lower-case RFC 4648 base32, the multibase `b` alphabet.
const BASE32_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// The CID transaction storage serves a preimage under, as a Bulletin node's `bitswap_v1_get`
/// takes it: CIDv1, the `raw` codec and the blake2b-256 multihash of the content, which is the
/// preimage key itself, in lower-case base32 with the multibase `b` prefix. Built from the
/// preimage key, or parsed from that text.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::From)]
#[display("{}", encode(_0))]
pub struct BulletinCid([u8; 32]);

impl BulletinCid {
	/// Whether `bytes` hash to this CID.
	pub fn matches(&self, bytes: &[u8]) -> bool {
		preimage_key(bytes) == self.0
	}
}

/// Why text is not a [`BulletinCid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum BulletinCidError {
	/// Not lower-case base32 with the multibase `b` prefix, spelled as a [`BulletinCid`] displays.
	#[display("the CID is not lower-case base32 with the multibase `b` prefix")]
	Encoding,
	/// Not a CIDv1 of raw bytes, such as the DAG-PB CID of a file stored in chunks.
	#[display("the CID is not a CIDv1 of raw bytes")]
	Codec,
	/// Hashed with something other than blake2b-256, such as SHA2-256.
	#[display("the CID's hash is not blake2b-256")]
	Hash,
	/// The digest is not 32 bytes long.
	#[display("the CID's digest is not 32 bytes")]
	Length,
}

impl FromStr for BulletinCid {
	type Err = BulletinCidError;

	fn from_str(cid: &str) -> Result<Self, Self::Err> {
		let mut bytes = Vec::new();
		let (mut buffer, mut bits) = (0u32, 0u32);
		for symbol in cid.strip_prefix('b').ok_or(BulletinCidError::Encoding)?.bytes() {
			let value = BASE32_ALPHABET
				.iter()
				.position(|&candidate| candidate == symbol)
				.ok_or(BulletinCidError::Encoding)?;
			buffer = (buffer << 5) | value as u32;
			bits += 5;
			if bits >= 8 {
				bits -= 8;
				bytes.push((buffer >> bits) as u8);
				buffer &= (1 << bits) - 1;
			}
		}
		let (cid_header, multihash_header) = PREIMAGE_CID_PREFIX.split_at(2);
		let key: [u8; 32] = bytes
			.strip_prefix(cid_header)
			.ok_or(BulletinCidError::Codec)?
			.strip_prefix(multihash_header)
			.ok_or(BulletinCidError::Hash)?
			.try_into()
			.map_err(|_| BulletinCidError::Length)?;
		if encode(&key) != cid {
			return Err(BulletinCidError::Encoding);
		}
		Ok(Self(key))
	}
}

fn encode(key: &[u8; 32]) -> String {
	let mut bytes = PREIMAGE_CID_PREFIX.to_vec();
	bytes.extend_from_slice(key);
	let mut cid = String::from("b");
	let (mut buffer, mut bits) = (0u32, 0u32);
	for byte in bytes {
		buffer = (buffer << 8) | u32::from(byte);
		bits += 8;
		while bits >= 5 {
			cid.push(char::from(BASE32_ALPHABET[((buffer >> (bits - 5)) & 31) as usize]));
			bits -= 5;
		}
	}
	if bits > 0 {
		cid.push(char::from(BASE32_ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
	}
	cid
}

#[cfg(test)]
mod tests;
