//! Unpacking a published executable's archive: a CARv1 file of UnixFS blocks.
//!
//! An archive comes in one of the two shapes the iOS host loads: a directory,
//! or a single file whose bytes are themselves a CAR of the directory, as an
//! uploaded CAR file is stored. Every block is checked against its CID before
//! it is read, so what is unpacked is what the archive's root names.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};

/// The `raw` codec: a block that is file bytes.
const RAW: u64 = 0x55;
/// The `dag-pb` codec: a UnixFS node.
const DAG_PB: u64 = 0x70;
/// The identity multihash: the digest is the data itself.
const IDENTITY: u64 = 0x00;
/// The SHA2-256 multihash.
const SHA2_256: u64 = 0x12;
/// The blake2b-256 multihash.
const BLAKE2B_256: u64 = 0xb220;
/// The most an archive unpacks to, so blocks linked many times over cannot
/// make it grow without bound.
const MAX_UNPACKED_BYTES: usize = 64 << 20;

/// A content identifier, by what it addresses rather than how it is spelled,
/// so a CIDv0 link finds the CIDv1 block of the same DAG-PB node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cid {
	codec: u64,
	hash: u64,
	digest: Vec<u8>,
}

impl Cid {
	/// Parse a binary CID at the start of `bytes`, answering it and the bytes
	/// after it.
	pub fn read(bytes: &[u8]) -> Result<(Self, &[u8])> {
		if let [0x12, 0x20, rest @ ..] = bytes {
			ensure!(rest.len() >= 32, "a CIDv0 is cut short");
			let (digest, rest) = rest.split_at(32);
			return Ok((Self { codec: DAG_PB, hash: SHA2_256, digest: digest.to_vec() }, rest));
		}
		let (version, rest) = varint(bytes)?;
		ensure!(version == 1, "CID version {version} is not supported");
		let (codec, rest) = varint(rest)?;
		let (hash, rest) = varint(rest)?;
		let (length, rest) = varint(rest)?;
		let length = usize::try_from(length).context("CID digest length")?;
		ensure!(rest.len() >= length, "a CID digest is cut short");
		let (digest, rest) = rest.split_at(length);
		Ok((Self { codec, hash, digest: digest.to_vec() }, rest))
	}

	/// Parse `bytes` as exactly one binary CID.
	pub fn parse(bytes: &[u8]) -> Result<Self> {
		let (cid, rest) = Self::read(bytes)?;
		ensure!(rest.is_empty(), "trailing bytes after a CID");
		Ok(cid)
	}

	/// The CIDv1 bytes.
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = vec![1];
		for value in [self.codec, self.hash, self.digest.len() as u64] {
			put_varint(&mut bytes, value);
		}
		bytes.extend_from_slice(&self.digest);
		bytes
	}

	/// The CIDv1 in lower-case base32 with the multibase `b` prefix, as a
	/// gateway path names it.
	pub fn to_base32(&self) -> String {
		const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
		let mut text = String::from("b");
		let (mut buffer, mut bits) = (0u32, 0u32);
		for byte in self.to_bytes() {
			buffer = (buffer << 8) | u32::from(byte);
			bits += 8;
			while bits >= 5 {
				bits -= 5;
				text.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
			}
			buffer &= (1 << bits) - 1;
		}
		if bits > 0 {
			text.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
		}
		text
	}

	/// Whether `data` hashes to this CID.
	fn verify(&self, data: &[u8]) -> Result<()> {
		let matches = match self.hash {
			IDENTITY => self.digest == data,
			SHA2_256 => self.digest == Sha256::digest(data).as_slice(),
			BLAKE2B_256 => self.digest == truapi::preimage_key(data),
			other => bail!("blocks hashed with multihash {other:#x} are not supported"),
		};
		ensure!(matches, "a block does not hash to its CID");
		Ok(())
	}
}

/// The files `car` holds, by path relative to its root, of either shape.
pub fn unpack(car: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
	let files = unpack_car(car)?;
	match files.get("") {
		Some(inner) if files.len() == 1 && looks_like_car(inner) => {
			unpack_car(inner).context("unpacking the CAR file the archive holds")
		},
		Some(_) => bail!("the archive is a single file, not a directory"),
		None => Ok(files),
	}
}

/// Whether `bytes` start with a CARv1 header naming a root.
pub fn looks_like_car(bytes: &[u8]) -> bool {
	header(bytes).is_ok()
}

/// The files `car`'s root holds, by path: one at `""` when the root is a file.
fn unpack_car(car: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
	let (root, mut rest) = header(car)?;
	let mut blocks = HashMap::new();
	while !rest.is_empty() {
		let (length, after) = varint(rest)?;
		let length = usize::try_from(length).context("block length")?;
		ensure!(after.len() >= length, "a block is cut short");
		let (section, after) = after.split_at(length);
		let (cid, data) = Cid::read(section)?;
		cid.verify(data)?;
		blocks.insert(cid, data);
		rest = after;
	}
	let mut tree = Tree { blocks, files: BTreeMap::new(), unpacked: 0 };
	tree.visit(&root, String::new())?;
	Ok(tree.files)
}

/// A CAR's blocks, and the files read out of them so far.
struct Tree<'a> {
	blocks: HashMap<Cid, &'a [u8]>,
	files: BTreeMap<String, Vec<u8>>,
	unpacked: usize,
}

impl Tree<'_> {
	/// Read the node `cid` names as the file or directory at `path`.
	fn visit(&mut self, cid: &Cid, path: String) -> Result<()> {
		let cid = cid.clone();
		let block = self.block_owned(&cid)?;
		if cid.codec == RAW {
			return self.add_file(path, block);
		}
		ensure!(cid.codec == DAG_PB, "codec {:#x} is not a UnixFS node", cid.codec);
		let node = PbNode::decode(&block)?;
		match node.unixfs()? {
			None | Some(UnixFs { kind: DIRECTORY, .. }) => {
				for link in node.links {
					let name = link.name.context("a directory entry has no name")?;
					ensure!(
						!name.is_empty() &&
							name != "." && name != ".." &&
							!name.contains(['/', '\\']),
						"directory entry {name:?} is not a plain name"
					);
					let child = if path.is_empty() { name } else { format!("{path}/{name}") };
					self.visit(&link.cid, child)?;
				}
				Ok(())
			},
			Some(UnixFs { kind: RAW_KIND | FILE, .. }) => {
				let mut content = Vec::new();
				self.file_content(&cid, &mut content)?;
				self.add_file(path, content)
			},
			Some(UnixFs { kind, .. }) => bail!("UnixFS node type {kind} is not supported"),
		}
	}

	/// The bytes of block `cid`, copied so the tree can be changed meanwhile.
	fn block_owned(&self, cid: &Cid) -> Result<Vec<u8>> {
		if cid.hash == IDENTITY {
			return Ok(cid.digest.clone());
		}
		self.blocks
			.get(cid)
			.map(|block| block.to_vec())
			.with_context(|| format!("the archive lacks block {}", cid.to_base32()))
	}

	/// Append the bytes of the file node `cid` to `content`: its own data,
	/// then each chunk it links, in order.
	fn file_content(&mut self, cid: &Cid, content: &mut Vec<u8>) -> Result<()> {
		let block = self.block_owned(cid)?;
		if cid.codec == RAW {
			return self.append(content, &block);
		}
		ensure!(cid.codec == DAG_PB, "codec {:#x} is not a file chunk", cid.codec);
		let node = PbNode::decode(&block)?;
		match node.unixfs()? {
			Some(UnixFs { kind: RAW_KIND | FILE, data }) => self.append(content, &data)?,
			_ => bail!("a file links a chunk that is not file data"),
		}
		for link in node.links {
			self.file_content(&link.cid, content)?;
		}
		Ok(())
	}

	fn append(&mut self, content: &mut Vec<u8>, data: &[u8]) -> Result<()> {
		self.unpacked += data.len();
		ensure!(self.unpacked <= MAX_UNPACKED_BYTES, "the archive unpacks to more than 64 MiB");
		content.extend_from_slice(data);
		Ok(())
	}

	fn add_file(&mut self, path: String, content: Vec<u8>) -> Result<()> {
		ensure!(self.files.insert(path.clone(), content).is_none(), "{path:?} appears twice");
		Ok(())
	}
}

/// The UnixFS `Raw` node type.
const RAW_KIND: u64 = 0;
/// The UnixFS `Directory` node type.
const DIRECTORY: u64 = 1;
/// The UnixFS `File` node type.
const FILE: u64 = 2;

/// A DAG-PB node: its links, and its UnixFS data when it has any.
struct PbNode {
	data: Option<Vec<u8>>,
	links: Vec<PbLink>,
}

/// One link of a DAG-PB node.
struct PbLink {
	cid: Cid,
	name: Option<String>,
}

/// The UnixFS fields read here.
struct UnixFs {
	kind: u64,
	data: Vec<u8>,
}

impl PbNode {
	fn decode(bytes: &[u8]) -> Result<Self> {
		let mut node = Self { data: None, links: Vec::new() };
		for field in fields(bytes)? {
			match field {
				(1, Field::Bytes(data)) => node.data = Some(data.to_vec()),
				(2, Field::Bytes(link)) => node.links.push(PbLink::decode(link)?),
				_ => {},
			}
		}
		Ok(node)
	}

	fn unixfs(&self) -> Result<Option<UnixFs>> {
		let Some(data) = &self.data else {
			return Ok(None);
		};
		let mut unixfs = UnixFs { kind: u64::MAX, data: Vec::new() };
		for field in fields(data)? {
			match field {
				(1, Field::Varint(kind)) => unixfs.kind = kind,
				(2, Field::Bytes(data)) => unixfs.data = data.to_vec(),
				_ => {},
			}
		}
		ensure!(unixfs.kind != u64::MAX, "a UnixFS node has no type");
		Ok(Some(unixfs))
	}
}

impl PbLink {
	fn decode(bytes: &[u8]) -> Result<Self> {
		let (mut cid, mut name) = (None, None);
		for field in fields(bytes)? {
			match field {
				(1, Field::Bytes(hash)) => cid = Some(Cid::parse(hash)?),
				(2, Field::Bytes(text)) => {
					name = Some(String::from_utf8(text.to_vec()).context("a link name")?)
				},
				_ => {},
			}
		}
		Ok(Self { cid: cid.context("a link has no hash")?, name })
	}
}

/// A protobuf field value.
enum Field<'a> {
	Varint(u64),
	Bytes(&'a [u8]),
	Fixed,
}

/// Every field of a protobuf message, by number.
fn fields(mut bytes: &[u8]) -> Result<Vec<(u64, Field<'_>)>> {
	let mut fields = Vec::new();
	while !bytes.is_empty() {
		let (key, rest) = varint(bytes)?;
		let (value, rest) = match key & 7 {
			0 => {
				let (value, rest) = varint(rest)?;
				(Field::Varint(value), rest)
			},
			1 => (Field::Fixed, rest.get(8..).context("a fixed64 field is cut short")?),
			2 => {
				let (length, rest) = varint(rest)?;
				let length = usize::try_from(length).context("field length")?;
				ensure!(rest.len() >= length, "a protobuf field is cut short");
				let (value, rest) = rest.split_at(length);
				(Field::Bytes(value), rest)
			},
			5 => (Field::Fixed, rest.get(4..).context("a fixed32 field is cut short")?),
			wire => bail!("protobuf wire type {wire} is not supported"),
		};
		fields.push((key >> 3, value));
		bytes = rest;
	}
	Ok(fields)
}

/// A CARv1 header's first root, and the bytes after the header.
fn header(car: &[u8]) -> Result<(Cid, &[u8])> {
	let (length, rest) = varint(car)?;
	let length = usize::try_from(length).context("header length")?;
	ensure!(rest.len() >= length, "the CAR header is cut short");
	let (header, rest) = rest.split_at(length);
	let mut cbor = Cbor(header);
	let entries = cbor.map()?;
	let (mut version, mut root) = (None, None);
	for _ in 0..entries {
		match cbor.text()? {
			"version" => version = Some(cbor.uint()?),
			"roots" => {
				let count = cbor.array()?;
				for index in 0..count {
					let cid = cbor.cid()?;
					if index == 0 {
						root = Some(cid);
					}
				}
			},
			_ => cbor.skip()?,
		}
	}
	ensure!(version == Some(1), "not a CARv1 archive");
	Ok((root.context("the CAR names no root")?, rest))
}

/// The little DAG-CBOR a CARv1 header is written in.
struct Cbor<'a>(&'a [u8]);

impl<'a> Cbor<'a> {
	/// The next item's major type and argument.
	fn head(&mut self) -> Result<(u8, u64)> {
		let (&first, rest) = self.0.split_first().context("the CAR header is cut short")?;
		let (major, info) = (first >> 5, first & 31);
		let size = match info {
			0..24 => 0,
			24 => 1,
			25 => 2,
			26 => 4,
			27 => 8,
			_ => bail!("indefinite-length CBOR is not DAG-CBOR"),
		};
		ensure!(rest.len() >= size, "the CAR header is cut short");
		let (argument, rest) = rest.split_at(size);
		self.0 = rest;
		let argument = if size == 0 {
			u64::from(info)
		} else {
			argument.iter().fold(0, |value, &byte| (value << 8) | u64::from(byte))
		};
		Ok((major, argument))
	}

	fn expect(&mut self, major: u8) -> Result<u64> {
		let (found, argument) = self.head()?;
		ensure!(found == major, "unexpected CBOR major type {found} in the CAR header");
		Ok(argument)
	}

	fn take(&mut self, length: u64) -> Result<&'a [u8]> {
		let length = usize::try_from(length).context("CBOR length")?;
		ensure!(self.0.len() >= length, "the CAR header is cut short");
		let (taken, rest) = self.0.split_at(length);
		self.0 = rest;
		Ok(taken)
	}

	fn uint(&mut self) -> Result<u64> {
		self.expect(0)
	}

	fn text(&mut self) -> Result<&'a str> {
		let length = self.expect(3)?;
		core::str::from_utf8(self.take(length)?).context("CBOR text")
	}

	fn array(&mut self) -> Result<u64> {
		self.expect(4)
	}

	fn map(&mut self) -> Result<u64> {
		self.expect(5)
	}

	/// A DAG-CBOR link: tag 42 around the CID bytes behind a zero byte.
	fn cid(&mut self) -> Result<Cid> {
		ensure!(self.expect(6)? == 42, "a CAR root is not a CID link");
		let length = self.expect(2)?;
		match self.take(length)? {
			[0, cid @ ..] => Cid::parse(cid),
			_ => bail!("a CAR root is not a DAG-CBOR CID"),
		}
	}

	/// Skip one item of any type a header can carry.
	fn skip(&mut self) -> Result<()> {
		let (major, argument) = self.head()?;
		match major {
			0 | 1 | 7 => Ok(()),
			2 | 3 => self.take(argument).map(drop),
			4 => (0..argument).try_for_each(|_| self.skip()),
			5 => (0..argument * 2).try_for_each(|_| self.skip()),
			_ => self.skip(),
		}
	}
}

/// An unsigned LEB128 varint at the start of `bytes`, and the bytes after it.
pub fn varint(bytes: &[u8]) -> Result<(u64, &[u8])> {
	let mut value = 0u64;
	for (index, &byte) in bytes.iter().enumerate().take(10) {
		value |= u64::from(byte & 0x7f) << (7 * index);
		if byte & 0x80 == 0 {
			return Ok((value, &bytes[index + 1..]));
		}
	}
	bail!("a varint is cut short or too long")
}

fn put_varint(bytes: &mut Vec<u8>, mut value: u64) {
	loop {
		let byte = (value & 0x7f) as u8;
		value >>= 7;
		if value == 0 {
			bytes.push(byte);
			return;
		}
		bytes.push(byte | 0x80);
	}
}

#[cfg(test)]
mod tests;
