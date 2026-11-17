use parity_scale_codec::Encode;
use scale_info::TypeInfo;
use truapi::{ProductId, v01::DerivationIndex};

use super::*;
use crate::host_logic::product_account::index_bytes;

fn decode_as<A, B>(source: A) -> B
where
	A: Encode + TypeInfo + 'static,
	B: DecodeAsType,
{
	let mut registry = scale_info::Registry::new();
	let type_id = registry.register_type(&scale_info::meta_type::<A>()).id;
	let types: scale_info::PortableRegistry = registry.into();
	B::decode_as_type(&mut &*source.encode(), type_id, &types).expect("dynamic decode succeeds")
}

/// The alias context hashes `product/{product_id}/` followed by the
/// suffix's 32-byte derivation index, so the alias and the product account
/// it belongs to are selected by the same value.
#[test]
fn context_bytes_pins_derivation_index_preimage() {
	let context = ProductProofContext {
		product_id: ProductId::for_test("voting.dot"),
		suffix: DerivationIndex::Index(0),
	};

	let mut expected = b"product/voting.dot/".to_vec();
	expected.extend_from_slice(&index_bytes(0));

	assert_eq!(context_bytes(&context), blake2b_256(&expected, None));
}

/// A raw 32-byte selector is used verbatim, and never collides with a
/// plain index.
#[test]
fn context_bytes_distinguishes_selector_forms() {
	let indexed = ProductProofContext {
		product_id: ProductId::for_test("voting.dot"),
		suffix: DerivationIndex::Index(0),
	};
	let raw = ProductProofContext {
		product_id: ProductId::for_test("voting.dot"),
		suffix: DerivationIndex::Raw([0; 32]),
	};

	assert_ne!(context_bytes(&indexed), context_bytes(&raw));
}

#[test]
fn missing_collection_is_not_a_ring_location() {
	let location = RingLocation {
		chain_id: [0; 32],
		junctions: vec![RingLocationJunction::PalletInstance(42)],
	};
	assert_eq!(collection_id(&location), Err(RingVrfError::RingNotFound));
}

#[test]
fn storage_projections_decode_runtime_shapes() {
	#[derive(Encode, TypeInfo)]
	enum SourceRingPosition {
		Onboarding { queue_page: u32, queued_at: u64 },
		Included { ring_index: u32, ring_page: u32, ring_position: u32 },
		Suspended,
	}
	#[derive(Encode, TypeInfo)]
	enum SourceRingExponent {
		R2e9,
		R2e10,
		R2e14,
	}
	#[derive(Encode, TypeInfo)]
	struct SourceCollectionInfo {
		owner: u8,
		mode: u8,
		ring_size: SourceRingExponent,
		self_inclusion_delay: Option<u64>,
	}
	#[derive(Encode, TypeInfo)]
	struct SourceBoundedMembers(Vec<[u8; 32]>);
	#[derive(Encode, TypeInfo)]
	struct SourceRingRoot {
		root: [u8; 4],
		revision: u32,
		intermediate: [u8; 8],
	}

	assert_eq!(
		decode_as::<_, RingPosition>(SourceRingPosition::Included {
			ring_index: 7,
			ring_page: 3,
			ring_position: 19,
		}),
		RingPosition::Included { ring_index: 7, ring_position: 19 }
	);
	assert_eq!(
		decode_as::<_, CollectionInfo>(SourceCollectionInfo {
			owner: 1,
			mode: 0,
			ring_size: SourceRingExponent::R2e10,
			self_inclusion_delay: Some(3_600),
		}),
		CollectionInfo { ring_size: RingExponent::R2e10 }
	);
	assert_eq!(
		decode_as::<_, BoundedMembers>(SourceBoundedMembers(vec![[0x11; 32], [0x22; 32]])),
		BoundedMembers(vec![[0x11; 32], [0x22; 32]])
	);
	assert_eq!(
		decode_as::<_, RingRoot>(SourceRingRoot {
			root: [0x33; 4],
			revision: 12,
			intermediate: [0x44; 8],
		}),
		RingRoot { revision: 12 }
	);

	// Keep every runtime variant in the type registry so this test also
	// checks their names remain aligned with the partial decoder.
	let _ = SourceRingPosition::Onboarding { queue_page: 0, queued_at: 0 };
	let _ = SourceRingPosition::Suspended;
	let _ = SourceRingExponent::R2e9;
	let _ = SourceRingExponent::R2e14;
}
