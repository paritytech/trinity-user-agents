//! The content of the `profile` text record.

use base64::{Engine, engine::general_purpose::STANDARD};
use parity_scale_codec::{Decode, DecodeAll, Encode};
use subxt::utils::WrapperKeepOpaque;

use crate::{BulletinCid, ImageFormat};

/// Largest record the core decodes, in bytes after base64.
const MAX_RECORD_BYTES: usize = 16 * 1024;

/// A person's public profile as published under their dotNS name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct ProfileRecord {
	/// In the owner's order.
	pub entries: Vec<WrapperKeepOpaque<ProfileEntry>>,
}

impl ProfileRecord {
	/// Decodes the base64 text a resolver holds. `None` when the text is not
	/// base64, exceeds the record size limit, or does not decode exactly.
	pub fn from_text(text: &str) -> Option<Self> {
		let bytes = STANDARD.decode(text.trim()).ok()?;
		if bytes.len() > MAX_RECORD_BYTES {
			return None;
		}
		Self::decode_all(&mut bytes.as_slice()).ok()
	}

	/// Encodes a publishable record. `None` if it exceeds the reader's size limit.
	pub fn to_text(&self) -> Option<String> {
		let bytes = self.encode();
		(bytes.len() <= MAX_RECORD_BYTES).then(|| STANDARD.encode(bytes))
	}

	/// Makes this avatar first while preserving every other entry and their relative order.
	pub fn set_avatar(&mut self, cid: BulletinCid, format: ImageFormat) {
		if let Some(index) = self
			.entries
			.iter()
			.position(|entry| matches!(entry.try_decode(), Some(ProfileEntry::Avatar(_))))
		{
			self.entries.remove(index);
		}
		self.entries.insert(
			0,
			WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar { cid, format })),
		);
	}

	/// The first `Avatar` entry.
	pub fn avatar(&self) -> Option<ProfileAvatar> {
		self.entries
			.iter()
			.find_map(|entry| entry.try_decode().map(|ProfileEntry::Avatar(avatar)| avatar))
	}
}

/// The kinds of entry a record may hold. More kinds will be added; a reader
/// keeps the ones it does not know as opaque bytes. Indices are never reused.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ProfileEntry {
	/// The person's avatar.
	#[codec(index = 0)]
	Avatar(ProfileAvatar),
}

/// Payload of an `Avatar` entry.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileAvatar {
	/// Bulletin CID of the image bytes, raw codec, blake2b-256.
	pub cid: BulletinCid,
	/// Declared by the writer.
	pub format: ImageFormat,
}

#[cfg(test)]
mod tests;
