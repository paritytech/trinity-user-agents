//! Ring parameters from the People chain (`Members` pallet).
//!
//! Reads the on-chain ring so the membership proof is built against the same
//! members the runtime verifies against: the baked-in `included` prefix of the
//! current ring. Every read is scoped to one [`PersonhoodCollection`], because
//! each collection is a separate ring with its own members and index.

use parity_scale_codec::Decode;
use scale_decode::DecodeAsType;
use sp_crypto_hashing::twox_128;
use thiserror::Error;

use super::{
	StatementAllowanceError,
	collection::PersonhoodCollection,
	extension::{Metadata, MetadataError},
	key_hash::{blake2_128_concat, twox_64_concat},
	rpc::RpcClient,
};

/// Error while reading or decoding ring storage.
#[derive(Debug, Error)]
pub enum RingError {
	/// Current ring index storage failed to decode.
	#[error("ring index: {0}")]
	RingIndex(#[source] parity_scale_codec::Error),
	/// Collection info was absent for the requested collection.
	#[error("Members.Collections[{collection}] missing")]
	CollectionMissing {
		/// Collection whose info was absent.
		collection: PersonhoodCollection,
	},
	/// Metadata-aware storage decode failed.
	#[error("{context}: {source}")]
	DecodeAsType {
		/// Decode context.
		context: &'static str,
		/// Metadata-aware decode failure.
		#[source]
		source: scale_decode::Error,
	},
	/// A ring key page failed to decode.
	#[error("ring keys page: {0}")]
	RingKeysPage(#[source] parity_scale_codec::Error),
	/// Ring status failed to decode.
	#[error("ring status: {0}")]
	RingStatus(#[source] parity_scale_codec::Error),
	/// Member has no `Members.Members` record for the collection.
	#[error("member has no Members.Members record for the collection")]
	MemberRecordMissing,
	/// Member is not included in a ring yet.
	#[error("member is not included in a ring (status: {status})")]
	MemberNotIncluded {
		/// Onboarding or suspended.
		status: &'static str,
	},
	/// Subscriber ring exponent was absent for the collection.
	#[error("MembersSubscriber.RingCollectionExponents missing for the collection")]
	SubscriberExponentMissing,
}

/// The leading fields of a `Members.RingKeysStatus` value.
#[derive(Decode)]
struct RingKeysStatus {
	_total: u32,
	included: u32,
}

/// Fields read from `Members.Collections`.
#[derive(Debug, PartialEq, Eq, DecodeAsType)]
pub(super) struct CollectionInfo {
	pub(super) ring_size: RingExponent,
}

/// Supported LitePeople ring domain sizes.
#[derive(Debug, PartialEq, Eq, DecodeAsType)]
pub(super) enum RingExponent {
	R2e9,
	R2e10,
	R2e14,
}

impl RingExponent {
	/// Return the exponent represented by the runtime enum variant.
	pub(super) fn exponent(self) -> u8 {
		match self {
			Self::R2e9 => 9,
			Self::R2e10 => 10,
			Self::R2e14 => 14,
		}
	}
}

/// Fields read from `Members.Root`.
#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct RingRoot {
	revision: u32,
}

/// On-chain ring parameters for building a verifying proof.
///
/// The `collection` names the ring these members came from. It must stay paired
/// with the entropy whose member key is in `members`: the extension declares the
/// collection, and the runtime verifies the proof against that collection's
/// ring, so a mismatched pair is rejected on chain.
#[derive(Debug)]
pub struct RingParams {
	/// Collection this ring belongs to.
	pub collection: PersonhoodCollection,
	/// Primary-first key ordinal within this collection's signer.
	/// Discovery, slot scanning and proof construction must use the same signer.
	pub key_index: usize,
	/// Ring members, sliced to the baked-in `included` prefix.
	pub members: Vec<[u8; 32]>,
	/// Ring size exponent (9 / 10 / 14).
	pub exponent: u8,
	/// Ring index these members belong to.
	pub ring_index: u32,
	/// Finalized block hash the ring snapshot was read at.
	pub block_hash: String,
}

/// `Members.CurrentRingIndex[id]` storage key.
pub(super) fn current_ring_index_key(collection: PersonhoodCollection) -> Vec<u8> {
	[
		twox_128(b"Members").as_slice(),
		twox_128(b"CurrentRingIndex").as_slice(),
		collection.identifier().as_slice(),
	]
	.concat()
}

/// `Members.Collections[id]` storage key.
pub(super) fn collections_key(collection: PersonhoodCollection) -> Vec<u8> {
	[
		twox_128(b"Members").as_slice(),
		twox_128(b"Collections").as_slice(),
		collection.identifier().as_slice(),
	]
	.concat()
}

/// `Members.RingKeysStatus[(id, ring_index)]` storage key.
pub(super) fn ring_keys_status_key(collection: PersonhoodCollection, ring_index: u32) -> Vec<u8> {
	[
		twox_128(b"Members").as_slice(),
		twox_128(b"RingKeysStatus").as_slice(),
		collection.identifier().as_slice(),
		&blake2_128_concat(&ring_index.to_le_bytes()),
	]
	.concat()
}

/// `Members.Root[(id, ring_index)]` storage key.
fn ring_root_key(collection: PersonhoodCollection, ring_index: u32) -> Vec<u8> {
	[
		twox_128(b"Members").as_slice(),
		twox_128(b"Root").as_slice(),
		collection.identifier().as_slice(),
		&blake2_128_concat(&ring_index.to_le_bytes()),
	]
	.concat()
}

/// `Members.RingKeys[(id, ring_index, page)]` storage key.
pub(super) fn ring_keys_key(
	collection: PersonhoodCollection,
	ring_index: u32,
	page: u32,
) -> Vec<u8> {
	[
		twox_128(b"Members").as_slice(),
		twox_128(b"RingKeys").as_slice(),
		collection.identifier().as_slice(),
		&blake2_128_concat(&ring_index.to_le_bytes()),
		&twox_64_concat(&page.to_le_bytes()),
	]
	.concat()
}

/// `Members.Members[(id, member)]` storage key.
/// The hashers are `Identity` then `Blake2_128Concat`.
fn member_record_key(collection: PersonhoodCollection, member: &[u8; 32]) -> Vec<u8> {
	[
		twox_128(b"Members").as_slice(),
		twox_128(b"Members").as_slice(),
		collection.identifier().as_slice(),
		&blake2_128_concat(member),
	]
	.concat()
}

/// `MembersSubscriber.RingCollectionExponents[id]` storage key.
/// It lives on the subscriber chain, Asset Hub.
fn subscriber_exponent_key(collection: PersonhoodCollection) -> Vec<u8> {
	[
		twox_128(b"MembersSubscriber").as_slice(),
		twox_128(b"RingCollectionExponents").as_slice(),
		&blake2_128_concat(collection.identifier()),
	]
	.concat()
}

/// Read the current ring index for `collection` at the current best block
/// (absent => 0).
pub async fn read_current_ring_index(
	rpc: &RpcClient,
	collection: PersonhoodCollection,
) -> Result<u32, StatementAllowanceError> {
	decode_ring_index(rpc.get_storage(&current_ring_index_key(collection)).await?)
}

/// Read the current ring index for `collection` pinned to block `at`
/// (absent => 0).
pub async fn read_current_ring_index_at(
	rpc: &RpcClient,
	collection: PersonhoodCollection,
	at: &str,
) -> Result<u32, StatementAllowanceError> {
	decode_ring_index(rpc.get_storage_at(&current_ring_index_key(collection), at).await?)
}

/// Decode a `CurrentRingIndex` storage value (absent => 0).
fn decode_ring_index(bytes: Option<Vec<u8>>) -> Result<u32, StatementAllowanceError> {
	match bytes {
		Some(bytes) => u32::decode(&mut &bytes[..]).map_err(|err| RingError::RingIndex(err).into()),
		None => Ok(0),
	}
}

/// Read the ring size exponent from `Collections[collection].ring_size`, pinned
/// to block `at`. This is a per-collection chain constant, so read it once and
/// reuse across ring indices.
pub async fn read_ring_exponent(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
	at: &str,
) -> Result<u8, StatementAllowanceError> {
	let info = rpc
		.get_storage_at(&collections_key(collection), at)
		.await?
		.ok_or(RingError::CollectionMissing { collection })?;
	let value_type = metadata
		.storage_value_type("Members", "Collections")
		.ok_or(MetadataError::MissingStorageType { pallet: "Members", entry: "Collections" })?;
	let mut input = info.as_slice();
	CollectionInfo::decode_as_type(&mut input, value_type, metadata.registry())
		.map(|info| info.ring_size.exponent())
		.map_err(|err| {
			RingError::DecodeAsType { context: "Members.Collections", source: err }.into()
		})
}

/// Read the members of `ring_index`, sliced to the baked-in `included`
/// prefix, with every read pinned to block `at` so pages and status come from
/// one consistent snapshot.
pub async fn read_ring_members_at(
	rpc: &RpcClient,
	collection: PersonhoodCollection,
	ring_index: u32,
	at: &str,
) -> Result<Vec<[u8; 32]>, StatementAllowanceError> {
	// 1. Page through RingKeys collecting raw 32-byte members.
	let mut members = Vec::new();
	for page in 0.. {
		let Some(bytes) =
			rpc.get_storage_at(&ring_keys_key(collection, ring_index, page), at).await?
		else {
			break;
		};
		let page = Vec::<[u8; 32]>::decode(&mut &bytes[..]).map_err(RingError::RingKeysPage)?;
		if page.is_empty() {
			break;
		}
		members.extend(page);
	}

	// 2. Slice to the baked-in `included` prefix (absent status => all included).
	if let Some(status) =
		rpc.get_storage_at(&ring_keys_status_key(collection, ring_index), at).await?
	{
		let status = RingKeysStatus::decode(&mut &status[..]).map_err(RingError::RingStatus)?;
		members.truncate(status.included as usize);
	}

	Ok(members)
}

/// Ring coordinates of one member. Projected from the runtime's `RingPosition`
/// enum.
#[derive(Debug, PartialEq, Eq, DecodeAsType)]
enum MemberRingPosition {
	/// Waiting in the onboarding queue.
	Onboarding {},
	/// Included in a built ring.
	Included { ring_index: u32 },
	/// Suspended from all rings.
	Suspended,
}

/// Reads the ring index `member` is included in for `collection`, from
/// `Members.Members`, pinned to block `at`. Errors when the member has no
/// record. Errors too when the member is not `Included` yet.
///
/// TODO(#334): second reader of `Members.Members` next to the subxt-typed one
/// in `signing_host/ring_vrf.rs`; converge them (they also differ on
/// non-`Included` members — this errors, that one skips).
pub async fn read_member_ring_index_at(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
	member: &[u8; 32],
	at: &str,
) -> Result<u32, StatementAllowanceError> {
	let value = rpc
		.get_storage_at(&member_record_key(collection, member), at)
		.await?
		.ok_or(RingError::MemberRecordMissing)?;
	let value_type = metadata
		.storage_value_type("Members", "Members")
		.ok_or(MetadataError::MissingStorageType { pallet: "Members", entry: "Members" })?;
	let mut input = value.as_slice();
	let position = MemberRingPosition::decode_as_type(&mut input, value_type, metadata.registry())
		.map_err(|err| RingError::DecodeAsType { context: "Members.Members", source: err })?;
	match position {
		MemberRingPosition::Included { ring_index, .. } => Ok(ring_index),
		MemberRingPosition::Onboarding {} => {
			Err(RingError::MemberNotIncluded { status: "onboarding" }.into())
		},
		MemberRingPosition::Suspended => {
			Err(RingError::MemberNotIncluded { status: "suspended" }.into())
		},
	}
}

/// Reads the ring exponent the subscriber chain, Asset Hub, verifies
/// `collection` against. The source is
/// `MembersSubscriber.RingCollectionExponents` at the current best block.
pub async fn read_subscriber_ring_exponent(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
) -> Result<u8, StatementAllowanceError> {
	let value = rpc
		.get_storage(&subscriber_exponent_key(collection))
		.await?
		.ok_or(RingError::SubscriberExponentMissing)?;
	let value_type = metadata
		.storage_value_type("MembersSubscriber", "RingCollectionExponents")
		.ok_or(MetadataError::MissingStorageType {
			pallet: "MembersSubscriber",
			entry: "RingCollectionExponents",
		})?;
	let mut input = value.as_slice();
	RingExponent::decode_as_type(&mut input, value_type, metadata.registry())
		.map(RingExponent::exponent)
		.map_err(|err| {
			RingError::DecodeAsType {
				context: "MembersSubscriber.RingCollectionExponents",
				source: err,
			}
			.into()
		})
}

/// Read `Members.Root[collection][ring_index].revision` pinned to block `at`
/// (absent => 0).
pub async fn read_ring_revision(
	rpc: &RpcClient,
	metadata: &Metadata,
	collection: PersonhoodCollection,
	ring_index: u32,
	at: &str,
) -> Result<u32, StatementAllowanceError> {
	match rpc.get_storage_at(&ring_root_key(collection, ring_index), at).await? {
		Some(bytes) => {
			let value_type = metadata
				.storage_value_type("Members", "Root")
				.ok_or(MetadataError::MissingStorageType { pallet: "Members", entry: "Root" })?;
			let mut input = bytes.as_slice();
			RingRoot::decode_as_type(&mut input, value_type, metadata.registry())
				.map(|root| root.revision)
				.map_err(|err| {
					RingError::DecodeAsType { context: "ring revision", source: err }.into()
				})
		},
		None => Ok(0),
	}
}

#[cfg(test)]
mod tests;
