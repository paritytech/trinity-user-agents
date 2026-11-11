use super::*;

const ALICE: [u8; 32] = [10u8; 32];
const BOB: [u8; 32] = [11u8; 32];

/// One identity's handle key, derived the way a live session's is.
fn key() -> [u8; 32] {
	handle_key_from_root_source(&[1u8; 32])
}

/// A second identity's, to stand in for another user.
fn other_key() -> [u8; 32] {
	handle_key_from_root_source(&[2u8; 32])
}

/// The resolver a host holds, for the same identity as [`key`].
fn handles() -> ContactHandles {
	ContactHandles::from_root_entropy_source(&[1u8; 32])
}

/// The other identity's resolver.
fn other_handles() -> ContactHandles {
	ContactHandles::from_root_entropy_source(&[2u8; 32])
}

#[test]
fn one_contact_has_one_handle_regardless_of_product() {
	// The point of the design: a handle is a durable shared id, so it takes
	// no product as input and cannot vary by caller.
	assert_eq!(contact_handle(&key(), &ALICE), contact_handle(&key(), &ALICE));
}

#[test]
fn two_contacts_have_different_handles() {
	assert_ne!(contact_handle(&key(), &ALICE), contact_handle(&key(), &BOB));
}

#[test]
fn two_users_have_different_handles_for_one_contact() {
	// Keyed per identity, so the same person is a different pseudonym to a
	// different user.
	assert_ne!(contact_handle(&key(), &ALICE), contact_handle(&other_key(), &ALICE));
}

#[test]
fn the_handle_key_is_not_the_root_source_it_came_from() {
	// Domain separation: the key is not the raw entropy source, so it cannot
	// collide with another use of the same secret.
	let source = [1u8; 32];
	assert_ne!(handle_key_from_root_source(&source), source);
}

#[test]
fn a_handle_does_not_reveal_the_account() {
	// Not a strength claim -- just that the account is not passed through.
	assert_ne!(contact_handle(&key(), &ALICE), ALICE);
}

#[test]
fn a_handle_names_the_account_it_was_minted_for() {
	let handle = contact_handle(&key(), &BOB);
	assert!(handles().names(&handle, &BOB));
}

#[test]
fn a_handle_names_no_other_account() {
	// A host answering with the wrong contact, or with somebody for a
	// handle a product invented, must be caught here.
	let handle = contact_handle(&key(), &BOB);
	assert!(!handles().names(&handle, &ALICE));
	assert!(!handles().names(&[0u8; 32], &BOB));
}

#[test]
fn a_handle_names_nobody_under_another_handle_key() {
	let handle = contact_handle(&key(), &BOB);
	assert!(!other_handles().names(&handle, &BOB));
}

#[test]
fn the_handle_matches_the_published_host_vector() {
	// Hosts compute handles themselves to answer a lookup, so the
	// derivation is a cross-language contract: BLAKE2b-256, keyed with the
	// handle key, over the account's 32 raw bytes. This vector is quoted in
	// the platform docs; it was produced by Python's `hashlib`, not by this
	// code.
	assert_eq!(
		hex::encode(contact_handle(&[0x11; 32], &[0x22; 32])),
		"d48c96fce9805f689b0bfa602feacdf3c7770d27e76c25d980eff0955e3714d2"
	);
}

#[test]
fn a_cached_handle_resolves_without_asking_the_host() {
	let cache = ContactHandleCache::default();
	let handle = contact_handle(&key(), &BOB);
	cache.insert(handle, BOB, cache.generation());
	assert_eq!(cache.get(&handle, &handles()), Some(BOB));
}

#[test]
fn a_cached_handle_from_another_key_is_a_miss() {
	// An entry left from an earlier session must not answer for this one.
	let cache = ContactHandleCache::default();
	let handle = contact_handle(&key(), &BOB);
	cache.insert(handle, BOB, cache.generation());
	assert_eq!(cache.get(&handle, &other_handles()), None);
}

#[test]
fn clearing_the_cache_forgets_every_handle() {
	let cache = ContactHandleCache::default();
	let handle = contact_handle(&key(), &BOB);
	cache.insert(handle, BOB, cache.generation());
	cache.clear();
	assert_eq!(cache.get(&handle, &handles()), None);
}

#[test]
fn a_full_cache_empties_rather_than_grows() {
	let cache = ContactHandleCache::default();
	for byte in 0..=u16::try_from(HANDLE_CACHE_MAX_ENTRIES).expect("cache size fits in u16") {
		let account = {
			let mut account = [0u8; 32];
			account[..2].copy_from_slice(&byte.to_le_bytes());
			account
		};
		cache.insert(contact_handle(&key(), &account), account, cache.generation());
	}
	assert_eq!(
		cache.state().entries.len(),
		1,
		"the entry that overflowed the bound is the only one left"
	);
}

#[test]
fn an_answer_from_before_a_clear_is_not_cached() {
	// The race a host lookup runs against a removal: the answer was read
	// before the host signalled, and must not be written back after.
	let cache = ContactHandleCache::default();
	let handle = contact_handle(&key(), &BOB);
	let before = cache.generation();
	cache.clear();
	cache.insert(handle, BOB, before);
	assert_eq!(cache.get(&handle, &handles()), None);
}

#[test]
fn a_known_handle_the_call_does_not_declare_is_caught() {
	let cache = ContactHandleCache::default();
	let handle = contact_handle(&key(), &BOB);
	cache.insert(handle, BOB, cache.generation());
	let mut call = vec![0x04, 0x00];
	call.extend_from_slice(&handle);

	assert!(cache.has_undeclared_handle(&call, &[]));
	assert!(!cache.has_undeclared_handle(&call, &[handle]));
	assert!(!cache.has_undeclared_handle(&[0x04, 0x00], &[]));
}

#[test]
fn a_picked_outcome_carries_nothing_but_a_handle() {
	// Encoded width pins the payload: one discriminant plus 32 bytes leaves
	// no room for a name, an account, or a count to ride along.
	let picked = truapi::latest::ContactPickOutcome::Picked {
		handle: truapi::latest::ContactHandle { bytes: contact_handle(&key(), &ALICE) },
	};
	assert_eq!(picked.encode().len(), 33);
}

#[test]
fn the_outcomes_that_disclose_nothing_encode_to_one_byte() {
	// Dismissed and NoContacts carry a discriminant and nothing else.
	assert_eq!(truapi::latest::ContactPickOutcome::Dismissed.encode().len(), 1);
	assert_eq!(truapi::latest::ContactPickOutcome::NoContacts.encode().len(), 1);
}
