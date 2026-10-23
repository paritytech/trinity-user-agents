//! Call arguments and authorizer extension values for allowance extrinsics,
//! which the core's `ExtrinsicService` assembles. Call arguments encode by
//! name against metadata, and the authorizers' variant indices are resolved
//! by name from the fetched runtime metadata, so a re-indexed runtime fails
//! loudly instead of encoding a wrong call.

use parity_scale_codec::{Decode, Encode};
use subxt::ext::scale_encode::EncodeAsType;

use super::{
	StatementAllowanceError,
	collection::PersonhoodCollection,
	extension::{AS_PGAS, Metadata},
};

/// `Option::Some` discriminant for the `AsResources` extension `extra`.
const OPTION_SOME: u8 = 0x01;

/// `Resources.set_statement_store_account` arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct SetStatementStoreAccountCallArgs {
	/// Statement-store period.
	pub period: u32,
	/// Slot within the period.
	pub seq: u32,
	/// Account the allowance is for.
	pub target_account: [u8; 32],
}

/// `Resources.claim_long_term_storage` arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ClaimLongTermStorageCallArgs {
	/// Long-term-storage period.
	pub period: u32,
	/// Claim counter within the period.
	pub counter: u8,
	/// Account the storage is for.
	pub account_id: [u8; 32],
}

/// `Pgas.claim_pgas` arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ClaimPgasCallArgs {
	/// PGAS slot of the day.
	pub slot_index: u32,
	/// Account credited.
	pub target: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct RegisterStatementStoreAllowanceInfo {
	proof: Vec<u8>,
	ring_index: u32,
	revision: u32,
	personhood: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ClaimLongTermStorageInfo {
	proof: Vec<u8>,
	ring_index: u32,
	revision: u32,
	personhood: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ClaimPgasInfo {
	proof: Vec<u8>,
	ring_index: u32,
	revision: u32,
	collection: u8,
	day: u32,
}

/// Encode the `AsResources` extension `extra` for a statement-store allowance:
/// `Some(RegisterStatementStoreAllowance { proof, ring_index, collection })`,
/// with the variant indices resolved from `metadata`.
pub fn build_as_resources_extra(
	metadata: &Metadata,
	proof: &[u8],
	ring_index: u32,
	revision: u32,
	collection: PersonhoodCollection,
) -> Result<Vec<u8>, StatementAllowanceError> {
	let (info_index, personhood) =
		metadata.as_resources_variant_indices("RegisterStatementStoreAllowance", collection)?;
	let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1);
	extra.push(OPTION_SOME);
	extra.push(info_index);
	RegisterStatementStoreAllowanceInfo { proof: proof.to_vec(), ring_index, revision, personhood }
		.encode_to(&mut extra);
	Ok(extra)
}

/// Encode the `AsResources` extension `extra` for a long-term storage claim:
/// `Some(ClaimLongTermStorage { proof, ring_index, revision, collection })`,
/// with the variant indices resolved from `metadata`.
pub fn build_long_term_storage_extra(
	metadata: &Metadata,
	proof: &[u8],
	ring_index: u32,
	revision: u32,
	collection: PersonhoodCollection,
) -> Result<Vec<u8>, StatementAllowanceError> {
	let (info_index, personhood) =
		metadata.as_resources_variant_indices("ClaimLongTermStorage", collection)?;
	let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1);
	extra.push(OPTION_SOME);
	extra.push(info_index);
	ClaimLongTermStorageInfo { proof: proof.to_vec(), ring_index, revision, personhood }
		.encode_to(&mut extra);
	Ok(extra)
}

/// Encode the `AsPgas` extension `extra` for a PGAS claim:
/// `Some(Claim { proof, ring_index, revision, collection, day })`, with the
/// variant indices resolved from `metadata`.
///
/// The runtime verifies the proof against the collection this declares, so it
/// must be the collection whose ring the proof was built in.
///
/// `AsPgas` names its membership enum `PgasCollection` rather than
/// `MembershipCollection`, so the tier is resolved by variant name.
pub fn build_as_pgas_extra(
	metadata: &Metadata,
	proof: &[u8],
	ring_index: u32,
	revision: u32,
	day: u32,
	collection: PersonhoodCollection,
) -> Result<Vec<u8>, StatementAllowanceError> {
	let (info_index, collection_index) = metadata.extension_info_and_field_variant_indices(
		AS_PGAS,
		"Claim",
		collection.metadata_variant(),
	)?;
	let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1 + 4);
	extra.push(OPTION_SOME);
	extra.push(info_index);
	ClaimPgasInfo {
		proof: proof.to_vec(),
		ring_index,
		revision,
		collection: collection_index,
		day,
	}
	.encode_to(&mut extra);
	Ok(extra)
}

#[cfg(test)]
mod tests {
	use parity_scale_codec::Compact;

	use super::{super::test_fixtures, *};

	const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");

	/// The collection the extension declares is what the runtime verifies the
	/// proof against, so each collection has to reach a distinct variant index
	/// resolved from metadata rather than a hardcoded one.
	///
	/// `AsPgas` lives on Asset Hub and is covered separately, against that
	/// chain's own fixture, by `a_full_person_pgas_claim_names_its_own_collection`.
	#[test]
	fn each_collection_reaches_its_own_extension_variant() {
		let metadata = Metadata::decode(FIXTURE).unwrap();
		let proof = vec![0xEE; 785];

		let mut encoded = Vec::new();
		for collection in PersonhoodCollection::ALL {
			let resources = build_as_resources_extra(&metadata, &proof, 3, 9, collection).unwrap();
			let long_term =
				build_long_term_storage_extra(&metadata, &proof, 3, 9, collection).unwrap();
			// The collection is the last byte of each `AsResources` payload.
			encoded.push((collection, *resources.last().unwrap(), *long_term.last().unwrap()));
		}

		let [people, lite] = <[_; 2]>::try_from(encoded).unwrap();
		assert_eq!(people.0, PersonhoodCollection::People);
		assert_eq!(lite.0, PersonhoodCollection::LitePeople);
		assert_eq!(
			(people.1, lite.1),
			(0x00, 0x01),
			"MembershipCollection declares People before LitePeople in the fixture"
		);
		assert_eq!((people.2, lite.2), (0x00, 0x01));
	}

	#[test]
	fn as_resources_extra_wraps_proof_as_bytes() {
		let metadata = Metadata::decode(FIXTURE).unwrap();
		let proof = vec![0xEE; 785];
		let extra =
			build_as_resources_extra(&metadata, &proof, 3, 9, PersonhoodCollection::LitePeople)
				.unwrap();
		// Some(0x01) ‖ variant(0x02) ‖ compact(785)=0x45,0x0c ‖ 785 bytes
		// ‖ ringIndex LE ‖ revision LE ‖ LitePeople.
		assert_eq!(
			extra,
			[
				vec![0x01, 0x02],
				Compact(785u32).encode(),
				proof,
				3u32.to_le_bytes().to_vec(),
				9u32.to_le_bytes().to_vec(),
				vec![0x01],
			]
			.concat()
		);
		assert_eq!(
			RegisterStatementStoreAllowanceInfo::decode(&mut &extra[2..]).unwrap(),
			RegisterStatementStoreAllowanceInfo {
				proof: vec![0xEE; 785],
				ring_index: 3,
				revision: 9,
				personhood: 1,
			}
		);
	}

	/// `AsPgas::Claim` carries a fifth field the `AsResources` claims do not, the
	/// day, and the variant indices come from Asset Hub rather than the relay.
	#[test]
	fn as_pgas_extra_wraps_proof_and_day() {
		let metadata = test_fixtures::asset_hub();
		let proof = vec![0xEE; 785];
		let extra =
			build_as_pgas_extra(metadata, &proof, 3, 9, 11, PersonhoodCollection::LitePeople)
				.unwrap();
		// Some(0x01) ‖ variant(0x00) ‖ compact(785)=0x45,0x0c ‖ 785 bytes
		// ‖ ringIndex LE ‖ revision LE ‖ LitePeople ‖ day LE.
		assert_eq!(
			extra,
			[
				vec![0x01, 0x00],
				Compact(785u32).encode(),
				proof,
				3u32.to_le_bytes().to_vec(),
				9u32.to_le_bytes().to_vec(),
				vec![0x01],
				11u32.to_le_bytes().to_vec(),
			]
			.concat()
		);
		assert_eq!(
			ClaimPgasInfo::decode(&mut &extra[2..]).unwrap(),
			ClaimPgasInfo {
				proof: vec![0xEE; 785],
				ring_index: 3,
				revision: 9,
				collection: 1,
				day: 11,
			}
		);
	}

	/// A full person proves PGAS against their own ring, so the claim has to name
	/// that collection. Until Asset Hub had a fixture this was only reachable
	/// live, which left the encoding of the full-person claim untested offline.
	#[test]
	fn a_full_person_pgas_claim_names_its_own_collection() {
		let metadata = test_fixtures::asset_hub();
		let proof = vec![0xEE; 785];

		let people =
			build_as_pgas_extra(metadata, &proof, 3, 9, 11, PersonhoodCollection::People).unwrap();
		let lite =
			build_as_pgas_extra(metadata, &proof, 3, 9, 11, PersonhoodCollection::LitePeople)
				.unwrap();

		assert_eq!(
			ClaimPgasInfo::decode(&mut &people[2..]).unwrap().collection,
			0,
			"Asset Hub declares People before LitePeople",
		);
		assert_eq!(ClaimPgasInfo::decode(&mut &lite[2..]).unwrap().collection, 1);
		assert_ne!(people, lite, "the collection has to reach the payload");
	}

	/// The `AsPgas::Claim` payload the live runtime declares. Encoding fewer
	/// fields than the runtime expects is accepted locally and then panics inside
	/// `validate_transaction`, which is how the missing `revision` on
	/// `RegisterStatementStoreAllowance` went unnoticed until a live submission.
	#[test]
	fn pgas_info_carries_the_five_declared_fields() {
		let info = ClaimPgasInfo {
			proof: vec![0xaa, 0xbb],
			ring_index: 7,
			revision: 9,
			collection: 1,
			day: 11,
		};

		assert_eq!(ClaimPgasInfo::decode(&mut &info.encode()[..]).unwrap(), info);
		// proof is length-prefixed; the four u32/u8 tail fields follow in order.
		assert_eq!(
			info.encode(),
			[vec![0x08, 0xaa, 0xbb], 7u32.encode(), 9u32.encode(), vec![1], 11u32.encode()]
				.concat(),
		);
	}

	#[test]
	fn pgas_call_encodes_slot_then_target() {
		let target = [0x33; 32];
		let args = ClaimPgasCallArgs { slot_index: 5, target };

		assert_eq!(args.encode(), [5u32.encode(), target.to_vec()].concat());
	}

	#[test]
	fn long_term_storage_extra_wraps_revision() {
		let metadata = Metadata::decode(FIXTURE).unwrap();
		let proof = vec![0xEE; 785];
		let extra = build_long_term_storage_extra(
			&metadata,
			&proof,
			3,
			9,
			PersonhoodCollection::LitePeople,
		)
		.unwrap();
		// Some(0x01) ‖ variant(0x03) ‖ compact(785)=0x45,0x0c ‖ proof
		// ‖ ringIndex LE ‖ revision LE ‖ LitePeople.
		assert_eq!(
			extra,
			[
				vec![0x01, 0x03],
				Compact(785u32).encode(),
				proof,
				3u32.to_le_bytes().to_vec(),
				9u32.to_le_bytes().to_vec(),
				vec![0x01],
			]
			.concat()
		);
		assert_eq!(
			ClaimLongTermStorageInfo::decode(&mut &extra[2..]).unwrap(),
			ClaimLongTermStorageInfo {
				proof: vec![0xEE; 785],
				ring_index: 3,
				revision: 9,
				personhood: 1,
			}
		);
	}
}

/// The typed call arguments encode against metadata by name; these pin the
/// bytes the removed positional encoders produced, which the runtime has
/// always received.
#[cfg(test)]
mod typed_call_tests {
	use subxt::{
		Metadata as SubxtMetadata,
		ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsFields},
	};

	use super::{super::test_fixtures, *};

	fn encode_typed(
		metadata_bytes: &[u8],
		pallet: &str,
		call: &str,
		args: &impl EncodeAsFields,
	) -> Vec<u8> {
		let metadata = SubxtMetadata::decode_from(metadata_bytes).unwrap();
		encode_call_data(pallet, call, args, &metadata, metadata.types()).unwrap()
	}

	#[test]
	fn the_statement_store_call_encodes_as_before() {
		let args =
			SetStatementStoreAccountCallArgs { period: 7, seq: 3, target_account: [0x22; 32] };
		let expected =
			"3f0a07000000030000002222222222222222222222222222222222222222222222222222222222222222";

		let encoded = encode_typed(
			test_fixtures::PEOPLE_METADATA,
			"Resources",
			"set_statement_store_account",
			&args,
		);

		assert_eq!(hex::encode(encoded), expected);
	}

	#[test]
	fn the_long_term_storage_call_encodes_as_before() {
		let args = ClaimLongTermStorageCallArgs { period: 9, counter: 2, account_id: [0x33; 32] };
		let expected =
			"3f0c09000000023333333333333333333333333333333333333333333333333333333333333333";

		let encoded = encode_typed(
			test_fixtures::PEOPLE_METADATA,
			"Resources",
			"claim_long_term_storage",
			&args,
		);

		assert_eq!(hex::encode(encoded), expected);
	}

	#[test]
	fn the_pgas_call_encodes_as_before() {
		let args = ClaimPgasCallArgs { slot_index: 4, target: [0x44; 32] };
		let expected =
			"6300040000004444444444444444444444444444444444444444444444444444444444444444";

		let encoded = encode_typed(test_fixtures::ASSET_HUB_METADATA, "Pgas", "claim_pgas", &args);

		assert_eq!(hex::encode(encoded), expected);
	}
}
