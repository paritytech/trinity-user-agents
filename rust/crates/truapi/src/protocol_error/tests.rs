use parity_scale_codec::Encode;

use super::*;

/// A later version of the protocol error needs no change to the decoder:
/// its errors are read from the type like V1's.
#[test]
fn the_errors_of_every_version_are_read_from_the_type() {
	#[derive(Encode, TypeInfo)]
	enum Versioned {
		V1(First),
		V2(Second),
	}
	#[derive(Encode, TypeInfo)]
	enum First {
		Only { id: u8 },
	}
	#[derive(Encode, TypeInfo)]
	enum Second {
		Only { id: u8 },
		Added { id: u8, version: u8 },
	}

	let payloads = [
		Versioned::V1(First::Only { id: 7 }).encode(),
		Versioned::V2(Second::Only { id: 7 }).encode(),
		Versioned::V2(Second::Added { id: 7, version: 2 }).encode(),
		// An error V1 does not have.
		vec![0, 1, 7],
		// An error V2 does not have.
		vec![1, 2, 7],
		// A version that does not exist.
		vec![2, 0, 7],
	];
	let (registry, type_id) = type_registry::<Versioned>();
	assert_eq!(
		payloads.map(|payload| indexes_are_known(&registry, type_id, &payload)),
		[true, true, true, false, false, false],
	);
}
