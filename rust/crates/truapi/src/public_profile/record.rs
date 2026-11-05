//! The content of the `profile` text record.

use base64::{Engine, engine::general_purpose::STANDARD};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use subxt::utils::WrapperKeepOpaque;

use crate::{BulletinCid, ImageFormat};

/// Largest record the core decodes, in bytes after base64.
const MAX_RECORD_BYTES: usize = 16 * 1024;

/// A person's public profile as published under their dotNS name.
///
/// Entries retain their encoded bytes so unknown kinds and extended payloads survive a rewrite.
#[derive(Debug, Clone, Default, PartialEq, Encode, Decode)]
pub struct ProfileRecord {
	/// In the owner's order.
	pub entries: Vec<WrapperKeepOpaque<ProfileEntry>>,
}

impl ProfileRecord {
	/// Decodes the resolver's base64 profile record.
	///
	/// Returns `None` for invalid base64, payloads larger than 16 KiB, invalid SCALE or trailing
	/// bytes.
	pub fn from_text(text: &str) -> Option<Self> {
		let bytes = STANDARD.decode(text.trim()).ok()?;
		if bytes.len() > MAX_RECORD_BYTES {
			return None;
		}
		Self::decode_all(&mut bytes.as_slice()).ok()
	}

	/// Returns the first decodable avatar.
	///
	/// Unknown or malformed entries are skipped. Returns `None` when no avatar decodes.
	pub fn avatar(&self) -> Option<ProfileAvatar> {
		self.entries
			.iter()
			.find_map(|entry| entry.try_decode().map(|ProfileEntry::Avatar(avatar)| avatar))
	}
}

/// A recognized kind of public profile entry.
///
/// Indices are never reused. Readers keep unknown kinds as opaque bytes in [`ProfileRecord`].
#[derive(Encode, Decode)]
pub enum ProfileEntry {
	/// The person's avatar.
	#[codec(index = 0)]
	Avatar(ProfileAvatar),
}

/// Payload of an `Avatar` entry.
///
/// The declaration does not validate the image bytes; the image loader checks them before use.
#[derive(Debug, PartialEq, Encode, Decode)]
pub struct ProfileAvatar {
	/// Bulletin CID of the image bytes, raw codec, blake2b-256.
	pub cid: BulletinCid,
	/// Declared by the writer.
	pub format: ImageFormat,
}

#[cfg(test)]
mod tests;
