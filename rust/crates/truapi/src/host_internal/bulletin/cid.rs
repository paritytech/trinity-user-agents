//! The CIDs Bulletin serves preimages under.

use core::str::FromStr;

use super::{BASE32_ALPHABET, PREIMAGE_CID_PREFIX, preimage_cid, preimage_key};

/// A Bulletin blob's CID: CIDv1, the raw codec and a blake2b-256 multihash, written as
/// [`preimage_cid`] writes it.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("{}", preimage_cid(_0))]
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
	/// Not lower-case base32 with the multibase `b` prefix, as [`preimage_cid`] writes it.
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
		if preimage_cid(&key) != cid {
			return Err(BulletinCidError::Encoding);
		}
		Ok(Self(key))
	}
}

#[cfg(test)]
mod tests;
