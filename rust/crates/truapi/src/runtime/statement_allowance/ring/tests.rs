use parity_scale_codec::Encode;
use scale_info::TypeInfo;
use subxt_rpcs::RpcClient as HostRpcClient;

use super::{super::rpc::testing::ScriptedRpc, *};

fn decode_as<Source, Target>(source: Source) -> Target
where
	Source: Encode + TypeInfo + 'static,
	Target: DecodeAsType,
{
	let mut registry = scale_info::Registry::new();
	let type_id = registry.register_type(&scale_info::meta_type::<Source>()).id;
	let registry: scale_info::PortableRegistry = registry.into();
	let encoded = source.encode();
	Target::decode_as_type(&mut encoded.as_slice(), type_id, &registry)
		.expect("source metadata is registered and target projection matches by field name; qed")
}

#[test]
fn ring_metadata_projections_ignore_unneeded_runtime_fields() {
	#[derive(Encode, TypeInfo)]
	enum SourceRingExponent {
		R2e14,
		R2e9,
		R2e10,
	}

	#[derive(Encode, TypeInfo)]
	struct SourceCollectionInfo {
		owner: u8,
		mode: u8,
		ring_size: SourceRingExponent,
		self_inclusion_delay: Option<u64>,
	}

	#[derive(Encode, TypeInfo)]
	struct SourceRingRoot {
		root: [u8; 4],
		revision: u32,
		intermediate: [u8; 8],
	}

	let collection: CollectionInfo = decode_as(SourceCollectionInfo {
		owner: 7,
		mode: 3,
		ring_size: SourceRingExponent::R2e10,
		self_inclusion_delay: Some(42),
	});
	let root: RingRoot =
		decode_as(SourceRingRoot { root: [0xaa; 4], revision: 12, intermediate: [0xbb; 8] });

	assert_eq!(collection, CollectionInfo { ring_size: RingExponent::R2e10 });
	assert_eq!(root, RingRoot { revision: 12 });

	// Keep every source variant in the metadata so index order differs
	// from the projection and variant-name decoding is exercised.
	let _ = SourceRingExponent::R2e14;
	let _ = SourceRingExponent::R2e9;

	// `Members.Members` value: the runtime's `RingPosition`, of which only
	// `Included.ring_index` is read.
	#[derive(Encode, TypeInfo)]
	enum SourceRingPosition {
		Onboarding { queue_page: u32, queued_at: u64 },
		Included { ring_index: u32, ring_page: u32, ring_position: u32 },
		Suspended,
	}
	assert_eq!(
		decode_as::<_, MemberRingPosition>(SourceRingPosition::Included {
			ring_index: 7,
			ring_page: 2,
			ring_position: 300,
		}),
		MemberRingPosition::Included { ring_index: 7 }
	);
	assert_eq!(
		decode_as::<_, MemberRingPosition>(SourceRingPosition::Onboarding {
			queue_page: 1,
			queued_at: 99,
		}),
		MemberRingPosition::Onboarding {}
	);
	assert_eq!(
		decode_as::<_, MemberRingPosition>(SourceRingPosition::Suspended),
		MemberRingPosition::Suspended
	);
}

#[test]
fn every_ring_key_is_scoped_to_its_collection() {
	// Without the identifier in the key, a People read would return the
	// LitePeople ring and the proof would be built against the wrong members.
	fn contains(haystack: &[u8], needle: &[u8]) -> bool {
		haystack.windows(needle.len()).any(|w| w == needle)
	}

	for (lite, full) in [
		(
			current_ring_index_key(PersonhoodCollection::LitePeople),
			current_ring_index_key(PersonhoodCollection::People),
		),
		(
			collections_key(PersonhoodCollection::LitePeople),
			collections_key(PersonhoodCollection::People),
		),
		(
			ring_keys_status_key(PersonhoodCollection::LitePeople, 4),
			ring_keys_status_key(PersonhoodCollection::People, 4),
		),
		(
			ring_root_key(PersonhoodCollection::LitePeople, 4),
			ring_root_key(PersonhoodCollection::People, 4),
		),
		(
			ring_keys_key(PersonhoodCollection::LitePeople, 4, 1),
			ring_keys_key(PersonhoodCollection::People, 4, 1),
		),
	] {
		assert_ne!(lite, full);
		assert!(
			contains(&lite, PersonhoodCollection::LitePeople.identifier()),
			"key does not carry the LitePeople identifier",
		);
		assert!(
			contains(&full, PersonhoodCollection::People.identifier()),
			"key does not carry the People identifier",
		);
	}
}

#[test]
fn member_reads_are_pinned_and_truncated_to_included() {
	// Page 0 holds two members; RingStatus { total: 2, included: 1, None }.
	let page = format!(r#""0x08{}{}""#, hex::encode([0xaa; 32]), hex::encode([0xbb; 32]),);
	let status = r#""0x020000000100000000""#;
	let scripted = ScriptedRpc::new([page.as_str(), "null", status]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let collection = PersonhoodCollection::LitePeople;
	let members =
		futures::executor::block_on(read_ring_members_at(&rpc, collection, 3, "0xat")).unwrap();

	assert_eq!(members, vec![[0xaa; 32]]);
	let expected: Vec<(String, String)> = [
		ring_keys_key(collection, 3, 0),
		ring_keys_key(collection, 3, 1),
		ring_keys_status_key(collection, 3),
	]
	.into_iter()
	.map(|key| ("state_getStorage".to_string(), format!(r#"["0x{}","0xat"]"#, hex::encode(key))))
	.collect();
	assert_eq!(scripted.calls(), expected);
}
