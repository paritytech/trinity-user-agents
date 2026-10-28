//! A person's public profile, as published under their dotNS name (RFC "Core-resolved
//! public profiles").
//!
//! The record is a list of typed entries. A reader decodes the kinds it knows and keeps the
//! rest, so a new kind never changes the codec. The bytes are what the writer (seity's
//! `profile-core`) encodes with the generated codec of these same types; the tests pin them.

use parity_scale_codec::{Decode, DecodeAll, Encode};

/// The dotNS text-record key a public profile lives under.
pub const PROFILE_TEXT_RECORD_KEY: &str = "profile";

/// Reader-side bound: a decoded record larger than this is not a profile.
pub const PROFILE_RECORD_MAX_BYTES: usize = 16 * 1024;

/// A display name is at most this many UTF-8 bytes.
pub const PROFILE_NAME_MAX_BYTES: usize = 64;

/// Entry kind of a [`ProfileAvatar`].
pub const PROFILE_ENTRY_AVATAR: &str = "avatar";
/// Entry kind of the display name: the raw UTF-8 bytes, no inner length prefix.
pub const PROFILE_ENTRY_NAME: &str = "name";
/// Entry kind of a [`ProfileNft`].
pub const PROFILE_ENTRY_NFT: &str = "nft";
/// Entry kind of a [`ProfileFeed`].
pub const PROFILE_ENTRY_FEED: &str = "feed";

/// A person's public profile as published under their dotNS name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileRecord {
	/// In the owner's order; a kind may repeat.
	pub entries: Vec<ProfileEntry>,
}

/// One element of a [`ProfileRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileEntry {
	/// `avatar`, `name`, `nft`, `feed`, or a kind this reader skips.
	pub kind: String,
	/// The kind's payload: SCALE for a tuple kind, bare bytes for `name`.
	pub payload: Vec<u8>,
}

/// Payload of an `avatar` entry.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileAvatar {
	/// Bulletin CID of the image bytes, raw codec, blake2b-256.
	pub cid: String,
	/// The image format, declared so no reader sniffs bytes.
	pub format: ProfileAvatarFormat,
}

/// The formats an avatar may be in. The index is what travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ProfileAvatarFormat {
	/// `image/png`.
	#[codec(index = 0)]
	Png,
	/// `image/jpeg`.
	#[codec(index = 1)]
	Jpeg,
	/// `image/webp`, what seity writes by default: half the bytes of JPEG at the same quality.
	#[codec(index = 2)]
	Webp,
}

/// Payload of an `nft` entry: one pinned item and the purse that should hold it. The chain,
/// not the record, says whether it is still there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ProfileNft {
	/// Scarcity's permanent instance id.
	pub instance: u64,
	/// Fixed at mint, so a reader can draw catalogue artwork before the instance is readable.
	pub collection: u32,
	/// Fixed at mint, as `collection`.
	pub item: u32,
	/// The account that should hold the item.
	pub purse: [u8; 32],
}

/// Payload of a `feed` entry: a statement-store feed the profile points at.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProfileFeed {
	/// The feed kind, such as `mood`.
	pub kind: String,
	/// The statement topic every statement of the feed carries.
	pub topic: [u8; 32],
	/// The account whose proof a statement must carry to count.
	pub signer: [u8; 32],
}

impl ProfileRecord {
	/// Decode a record exactly, refusing anything over [`PROFILE_RECORD_MAX_BYTES`] before
	/// parsing and anything with trailing bytes after it.
	pub fn decode_bounded(bytes: &[u8]) -> Result<Self, ProfileRecordError> {
		if bytes.len() > PROFILE_RECORD_MAX_BYTES {
			return Err(ProfileRecordError::TooLarge {
				bytes: bytes.len().try_into().unwrap_or(u32::MAX),
			});
		}
		Self::decode_all(&mut &bytes[..]).map_err(|_| ProfileRecordError::Malformed)
	}

	/// The first `avatar` entry that decodes to a format this reader knows. An avatar in an
	/// unknown format, or one whose payload is malformed, is no avatar.
	pub fn avatar(&self) -> Option<ProfileAvatar> {
		self.entries_of(PROFILE_ENTRY_AVATAR)
			.find_map(|payload| ProfileAvatar::decode_all(&mut &payload[..]).ok())
	}

	/// The first `name` entry that is valid UTF-8 of at most [`PROFILE_NAME_MAX_BYTES`].
	pub fn name(&self) -> Option<String> {
		self.entries_of(PROFILE_ENTRY_NAME)
			.filter(|payload| payload.len() <= PROFILE_NAME_MAX_BYTES)
			.find_map(|payload| String::from_utf8(payload.to_vec()).ok())
	}

	fn entries_of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a [u8]> + 'a {
		self.entries
			.iter()
			.filter(move |entry| entry.kind == kind)
			.map(|entry| entry.payload.as_slice())
	}
}

/// Why bytes are not a [`ProfileRecord`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileRecordError {
	/// Over the reader-side bound; refused before parsing.
	TooLarge {
		/// How many bytes there were.
		bytes: u32,
	},
	/// Not the SCALE encoding of a record, or not exactly one.
	Malformed,
}

#[cfg(test)]
mod tests {
	use super::*;

	// The bytes seity's profile-core writes for { avatar: (CID, webp), name: "Alice" }, where
	// the CID is raw/blake2b-256 of 0xab × 32 (profile-core/test/profile-envelope.test.ts).
	const VECTOR_HEX: &str = "08186176617461720101f86261666b32627a616365637632786b356c766f7632786b356c766f7632786b356c766f7632786b356c766f7632786b356c766f7632786b356c766f76327702106e616d6514416c696365";
	const VECTOR_CID: &str = "bafk2bzacecv2xk5lvov2xk5lvov2xk5lvov2xk5lvov2xk5lvov2xk5lvov2w";

	fn vector() -> Vec<u8> {
		hex::decode(VECTOR_HEX).expect("the vector is hex")
	}

	fn record(entries: Vec<(&str, Vec<u8>)>) -> ProfileRecord {
		ProfileRecord {
			entries: entries
				.into_iter()
				.map(|(kind, payload)| ProfileEntry { kind: kind.to_string(), payload })
				.collect(),
		}
	}

	#[test]
	fn decodes_the_bytes_the_writer_pins() {
		let record = ProfileRecord::decode_bounded(&vector()).expect("the vector decodes");
		assert_eq!(record.entries.len(), 2);
		assert_eq!(
			record.avatar(),
			Some(ProfileAvatar { cid: VECTOR_CID.to_string(), format: ProfileAvatarFormat::Webp })
		);
		assert_eq!(record.name(), Some("Alice".to_string()));
		assert_eq!(record.encode(), vector(), "re-encoding gives the writer's bytes back");
	}

	#[test]
	fn the_avatar_payload_is_a_scale_tuple_and_the_name_is_bare_bytes() {
		let avatar =
			ProfileAvatar { cid: VECTOR_CID.to_string(), format: ProfileAvatarFormat::Webp };
		let mut expected = parity_scale_codec::Compact(VECTOR_CID.len() as u32).encode();
		expected.extend_from_slice(VECTOR_CID.as_bytes());
		expected.push(2);
		assert_eq!(avatar.encode(), expected);

		let record = record(vec![("name", b"Alice".to_vec())]);
		assert_eq!(record.entries[0].payload, b"Alice", "no inner length prefix");
		assert_eq!(record.name(), Some("Alice".to_string()));
	}

	#[test]
	fn an_unknown_avatar_format_is_no_avatar_and_the_rest_still_reads() {
		let mut payload =
			ProfileAvatar { cid: VECTOR_CID.to_string(), format: ProfileAvatarFormat::Png }
				.encode();
		*payload.last_mut().unwrap() = 9;
		let record = record(vec![("avatar", payload), ("name", b"A".to_vec())]);
		assert_eq!(record.avatar(), None);
		assert_eq!(record.name(), Some("A".to_string()));
	}

	#[test]
	fn unknown_kinds_are_kept_and_the_first_of_a_kind_wins() {
		let second =
			ProfileAvatar { cid: VECTOR_CID.to_string(), format: ProfileAvatarFormat::Jpeg };
		let record = record(vec![
			("feed", vec![1, 2, 3]),
			("avatar", b"not a tuple".to_vec()),
			("avatar", second.clone().encode()),
			("name", b"First".to_vec()),
			("name", b"Second".to_vec()),
		]);
		let bytes = record.encode();
		let decoded = ProfileRecord::decode_bounded(&bytes).expect("decodes");
		assert_eq!(decoded.entries.len(), 5, "an unknown kind survives the round trip");
		assert_eq!(decoded.avatar(), Some(second), "a malformed avatar is skipped, not fatal");
		assert_eq!(decoded.name(), Some("First".to_string()));
	}

	#[test]
	fn a_name_over_64_bytes_or_not_utf8_is_no_name() {
		assert_eq!(record(vec![("name", vec![b'x'; 65])]).name(), None);
		assert_eq!(record(vec![("name", vec![0xff, 0xfe])]).name(), None);
		assert_eq!(
			record(vec![("name", "é".repeat(32).into_bytes())]).name(),
			Some("é".repeat(32))
		);
	}

	#[test]
	fn refuses_what_is_not_a_record() {
		let big = record(vec![("x", vec![0; PROFILE_RECORD_MAX_BYTES])]).encode();
		assert!(matches!(
			ProfileRecord::decode_bounded(&big),
			Err(ProfileRecordError::TooLarge { .. })
		));
		let truncated = &vector()[..vector().len() - 1];
		assert_eq!(ProfileRecord::decode_bounded(truncated), Err(ProfileRecordError::Malformed));
		let mut trailing = vector();
		trailing.push(0);
		assert_eq!(ProfileRecord::decode_bounded(&trailing), Err(ProfileRecordError::Malformed));
		assert_eq!(ProfileRecord::decode_bounded(&[]), Err(ProfileRecordError::Malformed));
	}
}
