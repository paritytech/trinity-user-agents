//! The content that a retention names: a CID that Bulletin has, the root block of a dag-pb DAG,
//! and the contenthash of a DotNS name.
//!
//! The cache nodes keep CIDv1 content with the codec raw or dag-pb and the hash sha2-256 or
//! blake2b-256. A bulletin-deploy site is a dag-pb UnixFS file node (sha2-256) that links raw
//! chunks. The DotNS contenthash of the site names that root.

use core::fmt;

use sha2::{Digest, Sha256};
use truapi::{
	host_logic::dotns_gateway::{call_bytes32, namehash_under},
	preimage_key,
};

use super::registry;

const VERSION: u64 = 1;
const RAW: u64 = 0x55;
const DAG_PB: u64 = 0x70;
const SHA2_256: u64 = 0x12;
const BLAKE2B_256: u64 = 0xb220;
const DIGEST_LENGTH: u64 = 32;
/// The multicodec of an IPFS contenthash (`ipfs-ns`).
const IPFS_NS: u64 = 0xe3;
const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
/// The DotNS content resolver of Paseo Next and Previewnet. It keeps the contenthash of each name.
const CONTENT_RESOLVER: [u8; 20] = [
	0x7f, 0x74, 0xd7, 0xcd, 0x50, 0xf5, 0xa8, 0x34, 0x27, 0x0e, 0x2a, 0xd3, 0x95, 0xa0, 0x1b, 0x01,
	0x89, 0x1a, 0xb3, 0x7d,
];

/// The reason for a CID that the cache nodes do not keep.
pub const UNSUPPORTED: &str =
	"the cache keeps raw and dag-pb content with sha2-256 or blake2b-256 hashes";

/// A CIDv1 that the cache nodes keep: the codec raw or dag-pb, and a 32-byte sha2-256 or
/// blake2b-256 digest. Its text is base32 with the multibase prefix `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cid {
	codec: u64,
	hash: u64,
	digest: [u8; 32],
}

impl Cid {
	/// The CID of a preimage key: raw, with the key as the blake2b-256 digest.
	pub fn preimage(key: [u8; 32]) -> Self {
		Self { codec: RAW, hash: BLAKE2B_256, digest: key }
	}

	/// The CID in `text`. A text that is not base32 CID bytes, or a CID that the cache nodes do
	/// not keep, gives the reason.
	pub fn parse(text: &str) -> Result<Self, String> {
		text.strip_prefix('b')
			.and_then(base32_decode)
			.ok_or_else(|| format!("{text} is not a CID in base32"))
			.and_then(|bytes| Self::from_bytes(&bytes))
	}

	/// The CID in the binary form `bytes`.
	pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
		let mut input = bytes;
		let mut next = || read_varint(&mut input).ok_or("the CID bytes are not valid");
		let (version, codec, hash, length) = (next()?, next()?, next()?, next()?);
		if version != VERSION ||
			!matches!(codec, RAW | DAG_PB) ||
			!matches!(hash, SHA2_256 | BLAKE2B_256) ||
			length != DIGEST_LENGTH
		{
			return Err(UNSUPPORTED.to_string());
		}
		let digest = <[u8; 32]>::try_from(input).map_err(|_| "the CID bytes are not valid")?;
		Ok(Self { codec, hash, digest })
	}

	/// Whether the CID names a dag-pb node.
	pub fn is_dag_pb(&self) -> bool {
		self.codec == DAG_PB
	}

	/// Whether the digest is blake2b-256. The cache nodes find and check blocks with this hash.
	pub fn is_blake2b(&self) -> bool {
		self.hash == BLAKE2B_256
	}

	/// The raw CID with the same multihash. Bulletin finds a block by its multihash, so this CID
	/// gives the block of a dag-pb CID too.
	pub fn raw(&self) -> Self {
		Self { codec: RAW, ..*self }
	}

	/// Whether `block` hashes to the digest of the CID.
	pub fn holds(&self, block: &[u8]) -> bool {
		let digest: [u8; 32] = match self.hash {
			SHA2_256 => Sha256::digest(block).into(),
			_ => preimage_key(block),
		};
		digest == self.digest
	}

	/// The binary form of the CID.
	pub fn to_bytes(self) -> Vec<u8> {
		let mut bytes = Vec::with_capacity(6 + self.digest.len());
		for value in [VERSION, self.codec, self.hash, DIGEST_LENGTH] {
			push_varint(&mut bytes, value);
		}
		bytes.extend_from_slice(&self.digest);
		bytes
	}
}

impl fmt::Display for Cid {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "b{}", base32_encode(&self.to_bytes()))
	}
}

/// Lower-case base32 (RFC 4648) with no pad characters.
fn base32_encode(bytes: &[u8]) -> String {
	let mut text = String::with_capacity(bytes.len() * 8 / 5 + 1);
	let (mut buffer, mut bits) = (0u32, 0u32);
	for byte in bytes {
		buffer = (buffer << 8) | u32::from(*byte);
		bits += 8;
		while bits >= 5 {
			text.push(char::from(BASE32[((buffer >> (bits - 5)) & 31) as usize]));
			bits -= 5;
		}
	}
	if bits > 0 {
		text.push(char::from(BASE32[((buffer << (5 - bits)) & 31) as usize]));
	}
	text
}

/// The bytes of lower-case base32 text with no pad characters, or `None` for a character outside
/// the alphabet.
fn base32_decode(text: &str) -> Option<Vec<u8>> {
	let mut bytes = Vec::with_capacity(text.len() * 5 / 8);
	let (mut buffer, mut bits) = (0u32, 0u32);
	for character in text.bytes() {
		let value = BASE32.iter().position(|letter| *letter == character)? as u32;
		buffer = (buffer << 5) | value;
		bits += 5;
		if bits >= 8 {
			bytes.push((buffer >> (bits - 8)) as u8);
			bits -= 8;
		}
	}
	Some(bytes)
}

/// The unsigned LEB128 varint at the start of `input`. The function moves `input` past it.
fn read_varint(input: &mut &[u8]) -> Option<u64> {
	let mut value = 0u64;
	for (index, byte) in input.iter().enumerate().take(10) {
		value |= u64::from(byte & 0x7f) << (7 * index);
		if byte & 0x80 == 0 {
			*input = &input[index + 1..];
			return Some(value);
		}
	}
	None
}

fn push_varint(bytes: &mut Vec<u8>, mut value: u64) {
	while value >= 0x80 {
		bytes.push((value as u8) | 0x80);
		value >>= 7;
	}
	bytes.push(value as u8);
}

/// One field of a protobuf message.
enum Field<'a> {
	Varint(u64),
	Bytes(&'a [u8]),
}

/// The fields of the protobuf message `message`, each with its field number. Only the wire types
/// varint (0) and length-delimited (2) occur in dag-pb.
fn fields(mut message: &[u8]) -> Result<Vec<(u64, Field<'_>)>, String> {
	let malformed = || "is not a valid dag-pb node".to_string();
	let mut fields = Vec::new();
	while !message.is_empty() {
		let key = read_varint(&mut message).ok_or_else(malformed)?;
		let field = match key & 7 {
			0 => Field::Varint(read_varint(&mut message).ok_or_else(malformed)?),
			2 => {
				let length = read_varint(&mut message).ok_or_else(malformed)?;
				let length = usize::try_from(length)
					.ok()
					.filter(|length| *length <= message.len())
					.ok_or_else(malformed)?;
				let (value, rest) = message.split_at(length);
				message = rest;
				Field::Bytes(value)
			},
			_ => return Err(malformed()),
		};
		fields.push((key >> 3, field));
	}
	Ok(fields)
}

/// The `Tsize` of each link of the dag-pb node `block`: the size of all blocks under that link.
/// A `PBNode` has its links in field 2. A `PBLink` has its `Tsize` in field 3.
pub fn link_sizes(block: &[u8]) -> Result<Vec<u64>, String> {
	fields(block)?
		.into_iter()
		.filter_map(|(number, field)| match (number, field) {
			(2, Field::Bytes(link)) => Some(link),
			_ => None,
		})
		.map(|link| {
			fields(link)?
				.into_iter()
				.find_map(|(number, field)| match (number, field) {
					(3, Field::Varint(size)) => Some(size),
					_ => None,
				})
				.ok_or_else(|| "has a link without a size".to_string())
		})
		.collect()
}

/// The ENS namehash of the dotted name `name`.
pub fn namehash(name: &str) -> [u8; 32] {
	name.rsplit('.').fold([0; 32], |parent, label| namehash_under(&parent, label))
}

/// The CID in the DotNS contenthash `contenthash`: `ipfs-ns` as a varint, then the CID bytes. An
/// empty contenthash gives `None`.
pub fn contenthash_cid(contenthash: &[u8]) -> Result<Option<Cid>, String> {
	if contenthash.is_empty() {
		return Ok(None);
	}
	let mut input = contenthash;
	if read_varint(&mut input) != Some(IPFS_NS) {
		return Err("the contenthash is not an IPFS CID".to_string());
	}
	Cid::from_bytes(input).map(Some)
}

/// The CID in the contenthash of the DotNS name `name`, from the view `contenthash(bytes32)` of
/// the content resolver, read through the Asset Hub JSON-RPC URL `rpc`. A name without a
/// contenthash gives `None`.
pub async fn contenthash(
	http: &reqwest::Client,
	rpc: &str,
	name: &str,
) -> Result<Option<Cid>, String> {
	let input = call_bytes32("contenthash(bytes32)", &namehash(name));
	let data = registry::view(http, rpc, &CONTENT_RESOLVER, &input, None).await?;
	// A call to an address without code succeeds and returns nothing.
	if data.is_empty() {
		return Err("the DotNS content resolver has no contract".to_string());
	}
	contenthash_cid(&registry::decode_bytes(&data)?)
}

#[cfg(test)]
mod tests {
	use truapi::preimage_cid;

	use super::*;
	use crate::cache_lookup::tests::{CACHE_DEMO_CID, CACHE_DEMO_ROOT};

	#[test]
	fn the_cache_demo_root_links_three_chunks_with_their_sizes() {
		let root = hex::decode(CACHE_DEMO_ROOT).unwrap();
		let cid = Cid::parse(CACHE_DEMO_CID).unwrap();
		assert_eq!(
			(
				root.len(),
				link_sizes(&root),
				cid.is_dag_pb(),
				cid.holds(&root),
				cid.raw().to_string(),
			),
			(
				154,
				Ok(vec![1573, 245_076, 910]),
				true,
				true,
				"bafkreicepvbzj6i4yflvozwz464yxasywzpf6uvabuz5ol2bs3johuoxxe".to_string()
			)
		);
	}

	// The size of a retention is the size of all blocks, so a link without a size cannot be paid.
	#[test]
	fn a_link_without_a_size_is_refused() {
		// PBNode { Links: [PBLink { Hash: 0x01, Name: "" }] }.
		let block = [0x12, 0x05, 0x0a, 0x01, 0x01, 0x12, 0x00];
		assert_eq!(link_sizes(&block), Err("has a link without a size".to_string()));
		assert!(link_sizes(&[0x12, 0x09]).is_err());
	}

	/// A preimage key and its CID text name the same content in both directions.
	#[test]
	fn preimage_cids_round_trip() {
		let key = preimage_key(b"content test");
		let text = preimage_cid(&key);
		assert_eq!(
			(Cid::preimage(key).to_string(), Cid::parse(&text)),
			(text, Ok(Cid::preimage(key)))
		);
	}

	#[test]
	fn other_cids_are_not_kept() {
		// dag-cbor (0x71) with sha2-256.
		let dag_cbor =
			format!("b{}", base32_encode(&[[1, 0x71, 0x12, 0x20].as_slice(), &[7; 32]].concat()));
		// raw with sha2-512 (0x13) of 64 bytes.
		let sha512 =
			format!("b{}", base32_encode(&[[1, 0x55, 0x13, 0x40].as_slice(), &[7; 64]].concat()));
		assert_eq!(
			[dag_cbor.as_str(), &sha512, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG", "b!"]
				.map(|text| Cid::parse(text).err()),
			[
				Some(UNSUPPORTED.to_string()),
				Some(UNSUPPORTED.to_string()),
				Some(
					"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG is not a CID in base32"
						.to_string()
				),
				Some("b! is not a CID in base32".to_string()),
			]
		);
	}

	#[test]
	fn a_contenthash_holds_an_ipfs_cid() {
		let cid = Cid::parse(CACHE_DEMO_CID).unwrap();
		let contenthash = [[0xe3, 0x01].as_slice(), &cid.to_bytes()].concat();
		assert_eq!(
			[
				contenthash_cid(&contenthash),
				contenthash_cid(&[]),
				contenthash_cid(&[0xe5, 0x01, 1])
			],
			[Ok(Some(cid)), Ok(None), Err("the contenthash is not an IPFS CID".to_string())]
		);
	}

	#[test]
	fn the_namehash_is_the_ens_namehash() {
		// The ENS namehash of `eth`, from EIP-137.
		assert_eq!(
			hex::encode(namehash("eth")),
			"93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae"
		);
		assert_eq!(namehash("site.paseo"), namehash_under(&namehash("paseo"), "site"));
	}
}
