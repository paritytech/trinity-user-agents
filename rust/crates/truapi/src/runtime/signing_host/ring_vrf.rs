//! Ring location resolution and Bandersnatch ring-VRF primitives.
//!
//! The resolver mirrors Nova's RFC-0004 implementation: it validates the
//! requested Members pallet from runtime metadata, pins every storage read to
//! one finalized block, and returns the ring members, exponent, and revision
//! from that snapshot.

use std::sync::Arc;

use crate::{
	chain_runtime::ChainRuntime,
	host_internal::sso_messages::RingVrfError,
	host_logic::product_account::derivation_index_bytes,
	runtime::vrf::{self, Vrf},
};
use subxt::{dynamic, ext::scale_decode::DecodeAsType};
use truapi::v01::{ProductProofContext, RingLocation, RingLocationJunction};

const MEMBERS_PALLET: &str = "Members";

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MemberCandidate {
	pub member: [u8; 32],
}

#[derive(Clone)]
pub struct ResolvedRing {
	pub selected: MemberCandidate,
	pub ring_index: u32,
	pub ring_revision: u32,
	pub domain_size: u32,
	pub members: Vec<[u8; 32]>,
}

#[async_trait::async_trait]
pub trait RingResolver: Send + Sync {
	/// Members pallet instance used in fully qualified ring locations.
	async fn members_pallet_index(&self, chain_id: &[u8; 32]) -> Result<u8, RingVrfError>;

	/// Validate the chain and Members pallet, returning the requested collection.
	async fn validate(&self, location: &RingLocation) -> Result<[u8; 32], RingVrfError>;

	/// Resolve a current, single-block ring snapshot and select the first
	/// candidate with an active membership.
	async fn resolve(
		&self,
		location: &RingLocation,
		candidates: &[MemberCandidate],
	) -> Result<ResolvedRing, RingVrfError>;
}

pub struct ChainRingResolver {
	chain: ChainRuntime,
}

impl ChainRingResolver {
	pub fn new(chain: ChainRuntime) -> Arc<Self> {
		Arc::new(Self { chain })
	}

	async fn at_ring(
		&self,
		location: &RingLocation,
	) -> Result<
		subxt::client::OnlineClientAtBlock<subxt::config::substrate::SubstrateConfig>,
		RingVrfError,
	> {
		let at_block = self.chain.current_block(&location.chain_id).await.map_err(unknown)?;
		let Some(pallet) = at_block.metadata_ref().pallet_by_name(MEMBERS_PALLET) else {
			return Err(RingVrfError::RingNotFound);
		};
		if let Some(expected) = pallet_instance(location) &&
			pallet.call_index() != expected
		{
			return Err(RingVrfError::RingNotFound);
		}
		Ok(at_block)
	}
}

#[async_trait::async_trait]
impl RingResolver for ChainRingResolver {
	async fn members_pallet_index(&self, chain_id: &[u8; 32]) -> Result<u8, RingVrfError> {
		let at_block =
			self.at_ring(&RingLocation { chain_id: *chain_id, junctions: vec![] }).await?;
		at_block
			.metadata_ref()
			.pallet_by_name(MEMBERS_PALLET)
			.map(|pallet| pallet.call_index())
			.ok_or(RingVrfError::RingNotFound)
	}

	async fn validate(&self, location: &RingLocation) -> Result<[u8; 32], RingVrfError> {
		self.at_ring(location).await?;
		collection_id(location)
	}

	async fn resolve(
		&self,
		location: &RingLocation,
		candidates: &[MemberCandidate],
	) -> Result<ResolvedRing, RingVrfError> {
		let collection = collection_id(location)?;
		let at_block = self.at_ring(location).await?;
		let storage = at_block.storage();

		let mut selected = None;
		for candidate in candidates {
			let address =
				dynamic::storage::<([u8; 32], [u8; 32]), RingPosition>(MEMBERS_PALLET, "Members");
			let Some(position) = storage
				.try_fetch(address, (collection, candidate.member))
				.await
				.map_err(unknown)?
			else {
				continue;
			};
			let RingPosition::Included { ring_index, ring_position, .. } =
				position.decode().map_err(unknown)?
			else {
				continue;
			};

			let status_address =
				dynamic::storage::<([u8; 32], u32), RingStatus>(MEMBERS_PALLET, "RingKeysStatus");
			let status = storage
				.fetch(status_address, (collection, ring_index))
				.await
				.map_err(unknown)?
				.decode()
				.map_err(unknown)?;
			if status.included > ring_position {
				selected = Some((*candidate, ring_index, status.included));
				break;
			}
		}

		let Some((selected, ring_index, included_count)) = selected else {
			return Err(RingVrfError::NotMember);
		};

		let collection_address =
			dynamic::storage::<([u8; 32],), CollectionInfo>(MEMBERS_PALLET, "Collections");
		let Some(collection_info) =
			storage.try_fetch(collection_address, (collection,)).await.map_err(unknown)?
		else {
			return Err(RingVrfError::RingNotFound);
		};
		let collection_info = collection_info.decode().map_err(unknown)?;

		let ring_keys_address =
			dynamic::storage::<([u8; 32], u32, u32), BoundedMembers>(MEMBERS_PALLET, "RingKeys");
		let mut pages = storage
			.iter(ring_keys_address, (collection, ring_index))
			.await
			.map_err(unknown)?;
		let mut members_by_page = Vec::new();
		while let Some(entry) = pages.next().await {
			let entry = entry.map_err(unknown)?;
			let page_index = entry
				.key()
				.map_err(unknown)?
				.part(2)
				.ok_or_else(|| RingVrfError::Unknown {
					reason: "Members.RingKeys returned a key without a page index".to_string(),
				})?
				.decode_as::<u32>()
				.map_err(unknown)?
				.ok_or_else(|| RingVrfError::Unknown {
					reason: "Members.RingKeys page index is not recoverable".to_string(),
				})?;
			let members = entry.value().decode().map_err(unknown)?.0;
			members_by_page.push((page_index, members));
		}
		members_by_page.sort_unstable_by_key(|(page, _)| *page);
		let mut members: Vec<_> =
			members_by_page.into_iter().flat_map(|(_, members)| members).collect();
		let included_count = usize::try_from(included_count).map_err(unknown)?;
		if members.len() < included_count {
			return Err(RingVrfError::Unknown {
				reason: format!(
					"Members.RingKeys contains {} keys but RingKeysStatus includes {included_count}",
					members.len()
				),
			});
		}
		members.truncate(included_count);
		if !members.contains(&selected.member) {
			return Err(RingVrfError::NotMember);
		}

		let root_address = dynamic::storage::<([u8; 32], u32), RingRoot>(MEMBERS_PALLET, "Root");
		let Some(root) = storage
			.try_fetch(root_address, (collection, ring_index))
			.await
			.map_err(unknown)?
		else {
			return Err(RingVrfError::RingNotFound);
		};
		let ring_revision = root.decode().map_err(unknown)?.revision;

		Ok(ResolvedRing {
			selected,
			ring_index,
			ring_revision,
			domain_size: collection_info.ring_size.domain_size(),
			members,
		})
	}
}

pub fn context_bytes(context: &ProductProofContext) -> [u8; 32] {
	let suffix = derivation_index_bytes(&context.suffix);
	let mut input = Vec::with_capacity(9 + context.product_id.as_str().len() + suffix.len());
	input.extend_from_slice(b"product/");
	input.extend_from_slice(context.product_id.as_str().as_bytes());
	input.push(b'/');
	input.extend_from_slice(&suffix);
	blake2b_256(&input, None)
}

pub fn create_proof(
	vrf: &Vrf,
	entropy: &[u8; 32],
	resolved: &ResolvedRing,
	context: &[u8],
	message: &[u8],
) -> Result<(Vec<u8>, [u8; 32]), RingVrfError> {
	vrf.prove(
		entropy,
		resolved.domain_size,
		&resolved.selected.member,
		&resolved.members,
		context,
		message,
	)
}

fn collection_id(location: &RingLocation) -> Result<[u8; 32], RingVrfError> {
	location
		.junctions
		.iter()
		.find_map(|junction| match junction {
			RingLocationJunction::CollectionId(value) => Some(value),
			RingLocationJunction::PalletInstance(_) => None,
		})
		.ok_or(RingVrfError::RingNotFound)?
		.as_slice()
		.try_into()
		.map_err(|_| RingVrfError::RingNotFound)
}

fn pallet_instance(location: &RingLocation) -> Option<u8> {
	location.junctions.iter().find_map(|junction| match junction {
		RingLocationJunction::PalletInstance(index) => Some(*index),
		RingLocationJunction::CollectionId(_) => None,
	})
}

fn blake2b_256(input: &[u8], key: Option<&[u8]>) -> [u8; 32] {
	let mut params = blake2b_simd::Params::new();
	params.hash_length(32);
	if let Some(key) = key {
		params.key(key);
	}
	let hash = params.hash(input);
	let mut output = [0u8; 32];
	output.copy_from_slice(hash.as_bytes());
	output
}

fn unknown(error: impl std::fmt::Debug) -> RingVrfError {
	RingVrfError::Unknown { reason: format!("{error:?}") }
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
enum RingPosition {
	Onboarding {},
	Included { ring_index: u32, ring_position: u32 },
	Suspended,
}

#[derive(DecodeAsType)]
struct RingStatus {
	included: u32,
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct CollectionInfo {
	ring_size: RingExponent,
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
enum RingExponent {
	R2e9,
	R2e10,
	R2e14,
}

impl RingExponent {
	fn domain_size(self) -> u32 {
		match self {
			Self::R2e9 => vrf::DOMAIN_2E11,
			Self::R2e10 => vrf::DOMAIN_2E12,
			Self::R2e14 => vrf::DOMAIN_2E16,
		}
	}
}

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct BoundedMembers(Vec<[u8; 32]>);

#[derive(Debug, PartialEq, Eq, DecodeAsType)]
struct RingRoot {
	revision: u32,
}

#[cfg(test)]
mod tests;
