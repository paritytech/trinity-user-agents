//! The personhood authorizers allowance extrinsics carry: transaction
//! extensions whose value proves ring membership over the extrinsic's
//! implication. Their values encode against the runtime's types by name.

use subxt::{ext::scale_encode::EncodeAsType, utils::H256};

use super::{
	PersonhoodSigner, StatementAllowanceError, collection::PersonhoodCollection, ring::RingParams,
};
use crate::chain::{
	ExtensionError, ExtrinsicBuildError, ExtrinsicService, InheritedImplication,
	TransactionExtension,
};

/// The core's extrinsic service and the chain an allowance extrinsic is built
/// for.
#[derive(Clone, Copy)]
pub struct ExtrinsicTarget<'a> {
	/// Builds the extrinsic.
	pub extrinsics: &'a ExtrinsicService,
	/// The chain's genesis as the host configured it.
	pub genesis: H256,
}

/// A ring-VRF membership proof over an extrinsic's implication: who proves,
/// in which ring, for which context, and at which ring root revision.
pub struct RingProof<'a> {
	/// Holds the personhood keys.
	pub signer: &'a dyn PersonhoodSigner,
	/// The ring the proof is built in.
	pub ring: &'a RingParams,
	/// The resource context the proof is bound to.
	pub context: [u8; 32],
	/// The ring root revision the proof is built against.
	pub revision: u32,
}

impl RingProof<'_> {
	/// The proof over the implication's hash.
	fn prove(&self, implication: &InheritedImplication) -> Result<Vec<u8>, ExtensionError> {
		self.signer
			.prove(self.ring, &self.context, &implication.hash())
			.map_err(ExtensionError::new)
	}
}

/// `AsResourcesInfo`: what an `AsResources` proof authorizes.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum AsResourcesInfo {
	/// A statement-store allowance: proof, ring index, revision, collection.
	RegisterStatementStoreAllowance(Vec<u8>, u32, u32, PersonhoodCollection),
	/// A long-term-storage claim: proof, ring index, revision, collection.
	ClaimLongTermStorage(Vec<u8>, u32, u32, PersonhoodCollection),
}

/// Which resource an `AsResources` proof claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourcesClaim {
	StatementStoreAllowance,
	LongTermStorage,
}

/// The People `AsResources` authorizer.
pub struct AsResourcesExtension<'a> {
	claim: ResourcesClaim,
	proof: RingProof<'a>,
}

impl<'a> AsResourcesExtension<'a> {
	/// Authorizes a statement-store allowance registration.
	pub fn statement_store_allowance(proof: RingProof<'a>) -> Self {
		Self { claim: ResourcesClaim::StatementStoreAllowance, proof }
	}

	/// Authorizes a long-term-storage claim.
	pub fn long_term_storage(proof: RingProof<'a>) -> Self {
		Self { claim: ResourcesClaim::LongTermStorage, proof }
	}
}

impl TransactionExtension for AsResourcesExtension<'_> {
	type Implicit = ();
	type Explicit = Option<AsResourcesInfo>;

	fn name(&self) -> &str {
		"AsResources"
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<Option<AsResourcesInfo>, ExtensionError> {
		let proof = self.proof.prove(implication)?;
		let ring = self.proof.ring;
		let (ring_index, revision) = (ring.ring_index, self.proof.revision);
		let info = match self.claim {
			ResourcesClaim::StatementStoreAllowance => {
				AsResourcesInfo::RegisterStatementStoreAllowance(
					proof,
					ring_index,
					revision,
					ring.collection,
				)
			},
			ResourcesClaim::LongTermStorage => {
				AsResourcesInfo::ClaimLongTermStorage(proof, ring_index, revision, ring.collection)
			},
		};
		Ok(Some(info))
	}
}

/// `AsPgasInfo`: what an `AsPgas` proof authorizes.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum AsPgasInfo {
	/// One day's PGAS claim.
	Claim {
		/// The ring proof.
		proof: Vec<u8>,
		/// The ring the proof is built in.
		ring_index: u32,
		/// The ring root revision the proof is built against.
		revision: u32,
		/// The collection whose ring it is.
		collection: PersonhoodCollection,
		/// The day claimed.
		day: u32,
	},
}

/// The Asset Hub `AsPgas` authorizer for one day's PGAS claim.
pub struct AsPgasExtension<'a> {
	proof: RingProof<'a>,
	day: u32,
}

impl<'a> AsPgasExtension<'a> {
	/// Authorizes a PGAS claim for `day`.
	pub fn new(proof: RingProof<'a>, day: u32) -> Self {
		Self { proof, day }
	}
}

impl TransactionExtension for AsPgasExtension<'_> {
	type Implicit = ();
	type Explicit = Option<AsPgasInfo>;

	fn name(&self) -> &str {
		"AsPgas"
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<Option<AsPgasInfo>, ExtensionError> {
		let ring = self.proof.ring;
		Ok(Some(AsPgasInfo::Claim {
			proof: self.proof.prove(implication)?,
			ring_index: ring.ring_index,
			revision: self.proof.revision,
			collection: ring.collection,
			day: self.day,
		}))
	}
}

impl From<ExtrinsicBuildError> for StatementAllowanceError {
	/// The authorizer's own error when its proof failed, else the build error.
	fn from(error: ExtrinsicBuildError) -> Self {
		let ExtrinsicBuildError::Extension { extension, source } = error else {
			return Self::Extrinsic(error);
		};
		source.downcast::<StatementAllowanceError>().unwrap_or_else(|source| {
			Self::Extrinsic(ExtrinsicBuildError::Extension { extension, source })
		})
	}
}

#[cfg(test)]
mod tests {
	use parity_scale_codec::Encode;
	use subxt::{Metadata, ext::frame_decode::extrinsics::ExtrinsicTypeInfo};

	use super::*;
	use crate::runtime::statement_allowance::test_fixtures::{ASSET_HUB_METADATA, PEOPLE_METADATA};

	/// Encodes `value` as the explicit value of `extension` in `metadata`'s
	/// pipeline 0.
	fn encode_explicit(
		metadata_bytes: &[u8],
		extension: &str,
		value: impl EncodeAsType,
	) -> Vec<u8> {
		let metadata = Metadata::decode_from(metadata_bytes).unwrap();
		let pipeline = metadata.extrinsic_extension_info(Some(0)).unwrap();
		let slot = pipeline
			.extension_ids
			.iter()
			.find(|declared| declared.name == extension)
			.unwrap();
		value.encode_as_type(slot.id, metadata.types()).unwrap()
	}

	/// `Some ‖ variant ‖ proof ‖ ring index ‖ revision ‖ LitePeople`, the
	/// positional layout People decodes.
	fn as_resources_bytes(variant: u8) -> Vec<u8> {
		[vec![0x01, variant], vec![0xEEu8; 4].encode(), 3u32.encode(), 9u32.encode(), vec![0x01]]
			.concat()
	}

	#[test]
	fn a_statement_store_allowance_encodes_as_people_expects() {
		let info = AsResourcesInfo::RegisterStatementStoreAllowance(
			vec![0xEE; 4],
			3,
			9,
			PersonhoodCollection::LitePeople,
		);

		let encoded = encode_explicit(PEOPLE_METADATA, "AsResources", Some(info));

		assert_eq!(encoded, as_resources_bytes(2));
	}

	#[test]
	fn a_long_term_storage_claim_encodes_as_people_expects() {
		let info = AsResourcesInfo::ClaimLongTermStorage(
			vec![0xEE; 4],
			3,
			9,
			PersonhoodCollection::LitePeople,
		);

		let encoded = encode_explicit(PEOPLE_METADATA, "AsResources", Some(info));

		assert_eq!(encoded, as_resources_bytes(3));
	}

	#[test]
	fn a_pgas_claim_encodes_as_asset_hub_expects() {
		let info = AsPgasInfo::Claim {
			proof: vec![0xEE; 4],
			ring_index: 3,
			revision: 9,
			collection: PersonhoodCollection::People,
			day: 11,
		};

		let encoded = encode_explicit(ASSET_HUB_METADATA, "AsPgas", Some(info));

		let expected = [
			vec![0x01, 0x00],
			vec![0xEEu8; 4].encode(),
			3u32.encode(),
			9u32.encode(),
			vec![0x00],
			11u32.encode(),
		]
		.concat();
		assert_eq!(encoded, expected);
	}
}
