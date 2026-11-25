use super::*;

#[test]
fn every_collection_identifier_is_distinct_and_exactly_32_bytes() {
	// The identifier is a fixed-width storage key, so a padding slip would
	// silently read a neighbouring collection rather than fail.
	for collection in PersonhoodCollection::ALL {
		assert_eq!(collection.identifier().len(), 32, "{collection}");
		assert!(collection.identifier().is_ascii(), "{collection} identifier is not ASCII");
	}
	assert_ne!(
		PersonhoodCollection::People.identifier(),
		PersonhoodCollection::LitePeople.identifier(),
	);
}

#[test]
fn the_people_identifier_is_the_padded_form_the_wallet_uses() {
	assert_eq!(
		PersonhoodCollection::People.identifier().as_slice(),
		b"pop:polkadot.network/people     ".as_slice(),
	);
	assert_eq!(&PersonhoodCollection::People.identifier()[..27], b"pop:polkadot.network/people",);
	// Padding is spaces, not zeroes: a zero-padded key addresses nothing.
	assert!(PersonhoodCollection::People.identifier()[27..].iter().all(|byte| *byte == b' '));
}

#[test]
fn people_is_offered_before_lite_people() {
	// Callers stop at the first collection that yields a slot, so ordering
	// is what makes a full person spend their wider budget first.
	assert_eq!(
		PersonhoodCollection::ALL,
		[PersonhoodCollection::People, PersonhoodCollection::LitePeople],
	);
}

#[test]
fn each_collection_names_its_own_pgas_claim_constant() {
	// Asset Hub declares a separate claim budget per collection, so a full
	// person must not be scanned against the light one's share.
	assert_eq!(
		PersonhoodCollection::People.pgas_claims_per_period_constant(),
		"MaxClaimsPerPeriodPerPerson",
	);
	assert_eq!(
		PersonhoodCollection::LitePeople.pgas_claims_per_period_constant(),
		"MaxClaimsPerPeriodPerLitePerson",
	);
	assert_ne!(
		PersonhoodCollection::People.pgas_claims_per_period_constant(),
		PersonhoodCollection::LitePeople.pgas_claims_per_period_constant(),
	);
}

#[test]
fn each_collection_names_its_own_variant_and_slot_view() {
	assert_eq!(PersonhoodCollection::People.to_string(), "People");
	assert_eq!(PersonhoodCollection::LitePeople.to_string(), "LitePeople");
	assert_eq!(
		PersonhoodCollection::People.slots_per_period_view(),
		"get_stmt_store_slots_per_period",
	);
	assert_eq!(
		PersonhoodCollection::LitePeople.slots_per_period_view(),
		"get_lite_stmt_store_slots_per_period",
	);
}
