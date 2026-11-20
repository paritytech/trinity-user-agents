//! The contact picker and the handles it hands out.
//!
//! A product never reads the accounts of the contact list. It opens the host's
//! picker, the host draws an overlay from its own chat contacts, and the core
//! turns the one person the user selected into a handle. Only the game
//! product, Jollity, reads the usernames of the contacts.
//!
//! The handle is **per product**: one contact yields a different value in each
//! product, so two products cannot join their users' contacts on it. Within a
//! product it is stable on every host of this user, because the key comes from
//! the session's root entropy source, which the wallet supplies alike to each
//! host, and from the product's dotNS identifier.
//!
//! No product can reach the root entropy source, so the mapping cannot be
//! recovered by hashing candidate accounts.

use std::{collections::HashMap, sync::Mutex};

use parity_scale_codec::Encode;

use crate::{host_logic::keyed_hash::blake2b_256_keyed, product_id::ProductId};

/// Upper bound on cached handles. The cache holds contacts the user picked, so
/// it stays small; reaching the bound empties it rather than evicting in order.
const HANDLE_CACHE_MAX_ENTRIES: usize = 256;

/// Domain separator for the contact-handle key.
pub const CONTACT_HANDLE_CONTEXT: &[u8] = b"truapi-contact-handle";

/// Domain-separate the session's root entropy source into the contact-handle
/// key, so this key cannot collide with another derived from the same source.
pub fn handle_key_from_root_source(root_entropy_source: &[u8; 32]) -> [u8; 32] {
	blake2b_256_keyed(root_entropy_source, CONTACT_HANDLE_CONTEXT)
}

/// Scope the user's handle key to one product, so the same contact hashes to an
/// unrelated handle in every other product.
pub fn product_handle_key(handle_key: &[u8; 32], product_id: &ProductId) -> [u8; 32] {
	blake2b_256_keyed(product_id.as_str().as_bytes(), handle_key)
}

/// The handle one contact is known by in one product, on every host of this
/// user.
///
/// `handle_key` is the product-scoped key from `product_handle_key`. It comes
/// from the session's root entropy source, which both host roles hold and no
/// product can reach. That is what makes this a real obfuscation rather than an
/// encoding: an adversary cannot recover the mapping by hashing candidate
/// accounts, because People-chain accounts are enumerable but the key is not
/// guessable.
pub fn contact_handle(handle_key: &[u8; 32], account: &[u8; 32]) -> [u8; 32] {
	blake2b_256_keyed(&account.encode(), handle_key)
}

/// The user's contact key, before it is scoped to a product.
///
/// Derived from the session's root entropy source, which both host roles hold
/// and no product can reach. It mints nothing itself: handles exist only per
/// product, through [`Self::for_product`], so no caller can hand out one
/// handle that two products share.
pub struct ContactKey([u8; 32]);

impl ContactKey {
	/// Derive the key from the session's root entropy source.
	pub fn from_root_entropy_source(root_entropy_source: &[u8; 32]) -> Self {
		Self(handle_key_from_root_source(root_entropy_source))
	}

	/// The handles `product_id` knows contacts by.
	pub fn for_product(&self, product_id: &ProductId) -> ContactHandles {
		ContactHandles {
			handle_key: product_handle_key(&self.0, product_id),
			product_id: product_id.clone(),
		}
	}
}

/// Both directions of one product's handle mapping: minting one for a contact
/// the user picked, and recovering the account a handle the product holds
/// names. Only a host can go either way.
pub struct ContactHandles {
	handle_key: [u8; 32],
	product_id: ProductId,
}

impl ContactHandles {
	/// The handle this product knows `account` by.
	pub fn mint(&self, account: &[u8; 32]) -> [u8; 32] {
		contact_handle(&self.handle_key, account)
	}

	/// The host lookup for the accounts `handles` name.
	///
	/// It carries the key the handles are minted under, which the host needs
	/// to look them up.
	pub fn lookup(&self, handles: Vec<[u8; 32]>) -> crate::platform::HostContactLookup {
		crate::platform::HostContactLookup { handle_key: self.handle_key, handles }
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
/// Shared by every product runtime of the host, so each entry records the
/// product it was minted for and answers only that product. Emptied whenever
/// the host signals its contacts changed, which is what keeps a removed contact
/// from resolving. Only the core writes it, so a handle it never minted cannot
/// be in it: a forged handle always misses and meets the host. Each hit is
/// re-minted under the current key before use, so an entry from an earlier
/// session is a miss rather than a wrong answer.
#[derive(Default)]
pub struct ContactHandleCache {
	state: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
	entries: HashMap<[u8; 32], CacheEntry>,
	/// Bumped by every clear. An answer obtained before a clear carries the
	/// older value and is dropped rather than cached, so a host lookup racing
	/// a removal cannot put the removed contact back.
	generation: u64,
}

struct CacheEntry {
	account: [u8; 32],
	product_id: ProductId,
}

impl ContactHandleCache {
	/// The current generation, to read before asking the host and pass to
	/// [`Self::insert`] with its answer.
	pub fn generation(&self) -> u64 {
		self.state().generation
	}

	/// Remember the account `handle` names under `handles`, unless the cache
	/// was cleared since `generation` was read.
	pub fn insert(
		&self,
		handles: &ContactHandles,
		handle: [u8; 32],
		account: [u8; 32],
		generation: u64,
	) {
		let mut state = self.state();
		if state.generation != generation {
			return;
		}
		if state.entries.len() >= HANDLE_CACHE_MAX_ENTRIES && !state.entries.contains_key(&handle) {
			state.entries.clear();
		}
		state
			.entries
			.insert(handle, CacheEntry { account, product_id: handles.product_id.clone() });
	}

	/// The account `handle` names under `handles`, if this cache holds it.
	pub fn get(&self, handle: &[u8; 32], handles: &ContactHandles) -> Option<[u8; 32]> {
		let state = self.state();
		let entry = state.entries.get(handle)?;
		(entry.product_id == handles.product_id && &handles.mint(&entry.account) == handle)
			.then_some(entry.account)
	}

	/// Whether `call` carries a handle this cache knows for `product_id` that
	/// `declared` does not list. Such a call would sign the handle bytes as an
	/// address nobody holds, so it is refused. Best effort: a handle minted
	/// before the last clear is no longer known here. Another product's
	/// handles are not looked at, so one product's picks never decide whether
	/// another's call goes through.
	pub fn has_undeclared_handle(
		&self,
		product_id: &ProductId,
		call: &[u8],
		declared: &[[u8; 32]],
	) -> bool {
		let state = self.state();
		if state.entries.is_empty() {
			return false;
		}
		call.windows(32).any(|window| {
			let window: [u8; 32] = window.try_into().expect("windows(32); qed");
			state.entries.get(&window).is_some_and(|entry| &entry.product_id == product_id) &&
				!declared.contains(&window)
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

#[cfg(test)]
mod tests;
