//! The contact picker and the handles it hands out.
//!
//! A product never reads the contact list. It opens the host's picker, the host
//! draws an overlay from its own chat contacts, and the core turns the one
//! person the user selected into a handle.
//!
//! The handle is deliberately **not** per-product: the same contact yields the
//! same value in every product and on every host of this user. Per-product
//! handles were considered and rejected — the core resolves a handle when it
//! builds a transaction anyway, so scoping bought no safety a product could not
//! route around, and it cost the durable shared id that makes the API usable and
//! that survives contacts syncing between a user's hosts.
//!
//! Keyed on the session's root entropy source, which no product can reach, so
//! the mapping cannot be recovered by hashing candidate accounts.

use std::{collections::HashMap, sync::Mutex};

use parity_scale_codec::Encode;

/// Upper bound on cached handles. The cache holds contacts the user picked, so
/// it stays small; reaching the bound empties it rather than evicting in order.
const HANDLE_CACHE_MAX_ENTRIES: usize = 256;

/// Domain separator for the contact-handle key.
pub const CONTACT_HANDLE_CONTEXT: &[u8] = b"truapi-contact-handle";

/// Domain-separate the session's root entropy source into the contact-handle
/// key, so this key cannot collide with another derived from the same source.
pub fn handle_key_from_root_source(root_entropy_source: &[u8; 32]) -> [u8; 32] {
	blake2b256_keyed(root_entropy_source, CONTACT_HANDLE_CONTEXT)
}

/// The handle one contact is known by, across every product and every host of
/// this user.
///
/// `handle_key` comes from the session's root entropy source, which both host
/// roles hold and no product can reach. That is what makes this a real
/// obfuscation rather than an encoding: an adversary cannot recover the mapping
/// by hashing candidate accounts, because People-chain accounts are enumerable
/// but the key is not guessable.
///
/// Product-independent on purpose — one contact is one handle everywhere, and
/// the key is uniform across a user's hosts because the wallet supplies the same
/// root source to each.
pub fn contact_handle(handle_key: &[u8; 32], account: &[u8; 32]) -> [u8; 32] {
	blake2b256_keyed(&account.encode(), handle_key)
}

/// Both directions of the handle mapping: minting one for a contact the user
/// picked, and recovering the account a handle a product holds names.
///
/// Keyed on the session's root entropy source, which both host roles hold and
/// no product can reach, so only a host can go either way.
pub struct ContactHandles {
	handle_key: [u8; 32],
}

impl ContactHandles {
	/// Derive the handle key from the session's root entropy source.
	#[cfg(test)]
	pub fn from_root_entropy_source(root_entropy_source: &[u8; 32]) -> Self {
		Self::from_handle_key(handle_key_from_root_source(root_entropy_source))
	}

	/// Take the handle key an authority already derived.
	pub fn from_handle_key(handle_key: [u8; 32]) -> Self {
		Self { handle_key }
	}

	/// The handle this user knows `account` by.
	pub fn mint(&self, account: &[u8; 32]) -> [u8; 32] {
		contact_handle(&self.handle_key, account)
	}

	/// The key handles are minted under, which a host needs to look them up.
	pub fn handle_key(&self) -> [u8; 32] {
		self.handle_key
	}

	/// Whether `account` is the one `handle` names.
	///
	/// Run on every account a host returns, so a host that answers with the
	/// wrong contact, or with anyone at all for a made-up handle, is refused
	/// rather than trusted: resolution is the host's lookup, but the check
	/// stays in the core.
	pub fn names(&self, handle: &[u8; 32], account: &[u8; 32]) -> bool {
		&self.mint(account) == handle
	}
}

/// Why the handles a transaction declares could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactResolutionError {
	/// The host serves no contacts platform.
	Unsupported,
	/// No active session, so there is no handle key.
	NotConnected,
	/// The host's lookup failed or answered out of shape. Says nothing about
	/// whether the contact is current, so a product can retry.
	Host(String),
	/// A handle names no current contact, is missing from the call it was
	/// declared for, or is in the call without being declared.
	UnknownContact,
}

/// Handles this host minted or resolved, mapped back to their accounts, so a
/// transaction naming one needs no host lookup.
///
/// Emptied whenever the host signals its contacts changed, which is what keeps
/// a removed contact from resolving. Only the core writes it, so a handle it
/// never minted cannot be in it: a forged handle always misses and meets the
/// host. Each hit is re-minted under the current key before use, so an entry
/// from an earlier session is a miss rather than a wrong answer.
#[derive(Default)]
pub struct ContactHandleCache {
	state: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
	entries: HashMap<[u8; 32], [u8; 32]>,
	/// Bumped by every clear. An answer obtained before a clear carries the
	/// older value and is dropped rather than cached, so a host lookup racing
	/// a removal cannot put the removed contact back.
	generation: u64,
}

impl ContactHandleCache {
	/// The current generation, to read before asking the host and pass to
	/// [`Self::insert`] with its answer.
	pub fn generation(&self) -> u64 {
		self.state().generation
	}

	/// Remember the account `handle` names, unless the cache was cleared since
	/// `generation` was read.
	pub fn insert(&self, handle: [u8; 32], account: [u8; 32], generation: u64) {
		let mut state = self.state();
		if state.generation != generation {
			return;
		}
		if state.entries.len() >= HANDLE_CACHE_MAX_ENTRIES && !state.entries.contains_key(&handle) {
			state.entries.clear();
		}
		state.entries.insert(handle, account);
	}

	/// The account `handle` names under `handles`, if this cache holds it.
	pub fn get(&self, handle: &[u8; 32], handles: &ContactHandles) -> Option<[u8; 32]> {
		let account = *self.state().entries.get(handle)?;
		(&handles.mint(&account) == handle).then_some(account)
	}

	/// Whether `call` carries a handle this cache knows that `declared` does
	/// not list. Such a call would sign the handle bytes as an address nobody
	/// holds, so it is refused. Best effort: a handle minted before the last
	/// clear is no longer known here.
	pub fn has_undeclared_handle(&self, call: &[u8], declared: &[[u8; 32]]) -> bool {
		let state = self.state();
		if state.entries.is_empty() {
			return false;
		}
		call.windows(32).any(|window| {
			let window: [u8; 32] = window.try_into().expect("windows(32); qed");
			state.entries.contains_key(&window) && !declared.contains(&window)
		})
	}

	/// Forget every entry, so the next resolution asks the host.
	pub fn clear(&self) {
		let mut state = self.state();
		state.entries.clear();
		state.generation += 1;
	}

	fn state(&self) -> std::sync::MutexGuard<'_, CacheState> {
		self.state.lock().expect("contact handle cache poisoned")
	}
}

fn blake2b256_keyed(message: &[u8], key: &[u8]) -> [u8; 32] {
	blake2b_simd::Params::new()
		.hash_length(32)
		.key(key)
		.hash(message)
		.as_bytes()
		.try_into()
		.expect("hash_length(32) configures BLAKE2b output to exactly 32 bytes; qed")
}

#[cfg(test)]
mod tests {
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
		for byte in 0..=HANDLE_CACHE_MAX_ENTRIES as u16 {
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
	fn the_product_wire_surface_is_the_picker_and_nothing_else() {
		// The contact list must not be reachable from a product. This asserts
		// the dispatch table itself, so adding a list or subscribe method to the
		// `Contacts` trait fails here rather than shipping.
		let contacts: Vec<&str> = crate::generated::wire_table::WIRE_TABLE
			.iter()
			.map(|entry| entry.method)
			.filter(|method| method.starts_with("contacts_"))
			.collect();
		assert_eq!(contacts, vec!["contacts_pick"]);
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
}
