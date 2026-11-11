//! The contact picker and the handles it hands out.
//!
//! A product never reads the contact list. It opens the host's picker, the host
//! draws an overlay from its own chat contacts, and the core turns the
//! confirmed selection into handles.
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

use std::{
	collections::{HashMap, HashSet},
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
};

use parity_scale_codec::Encode;

/// Upper bound on cached handles. The cache holds contacts the user picked, so
/// it stays small; reaching the bound empties it rather than evicting in order.
const HANDLE_CACHE_MAX_ENTRIES: usize = 256;
const MAX_CONTACTS: usize = 256;

/// Bound and deduplicate product-supplied selections without changing order.
pub fn selected_handles(selected: Vec<crate::latest::ContactHandle>) -> Option<Vec<[u8; 32]>> {
	if selected.len() > MAX_CONTACTS {
		return None;
	}
	let mut seen = HashSet::with_capacity(selected.len());
	Some(
		selected
			.into_iter()
			.map(|handle| handle.bytes)
			.filter(|handle| seen.insert(*handle))
			.collect(),
	)
}

/// Bound and deduplicate accounts a host picker confirmed.
pub fn selected_accounts(mut accounts: Vec<[u8; 32]>) -> Option<Vec<[u8; 32]>> {
	if accounts.len() > MAX_CONTACTS {
		return None;
	}
	let mut seen = HashSet::with_capacity(accounts.len());
	accounts.retain(|account| seen.insert(*account));
	Some(accounts)
}

/// Validate only product-controlled geometry, never contact availability.
pub fn valid_label_placement(request: &crate::latest::HostContactsPlaceLabelsRequest) -> bool {
	const MAX_SIDE: u32 = 16384;
	if !(1..=MAX_SIDE).contains(&request.surface_width) ||
		!(1..=MAX_SIDE).contains(&request.surface_height) ||
		request.slots.len() > MAX_CONTACTS
	{
		return false;
	}
	let mut seen = HashSet::with_capacity(request.slots.len());
	request.slots.iter().all(|slot| {
		seen.insert(slot.slot) &&
			(1..=MAX_SIDE).contains(&slot.rect.width) &&
			(1..=MAX_SIDE).contains(&slot.rect.height) &&
			slot.clip.width <= MAX_SIDE &&
			slot.clip.height <= MAX_SIDE
	})
}

/// Connection-owned label layer, serialized so delayed draws cannot overtake clears.
pub struct ContactLabelPlacement {
	platform: Arc<dyn crate::platform::ContactsPlatform>,
	product: crate::platform::ProductContext,
	/// Last submitted width, height and directory generation, held across lookup and draw.
	pub surface: futures::lock::Mutex<Option<(u32, u32, u64)>>,
	closed: AtomicBool,
}

impl ContactLabelPlacement {
	/// Whether teardown has started for this connection.
	pub fn is_closed(&self) -> bool {
		self.closed.load(Ordering::Acquire)
	}

	async fn clear(&self, invalidated_before: Option<u64>) {
		let mut surface = self.surface.lock().await;
		if let (Some((_, _, generation)), Some(invalidated_before)) = (*surface, invalidated_before) &&
			generation >= invalidated_before
		{
			return;
		}
		if let Some((surface_width, surface_height, _)) = surface.take() {
			let _ = self
				.platform
				.place_contact_labels(
					&self.product,
					crate::platform::PlacedContactLabels {
						surface_width,
						surface_height,
						labels: Vec::new(),
					},
				)
				.await;
		}
	}
}

/// Host-owned labels belonging to live product connections.
#[derive(Default)]
pub struct ContactLabelPlacements {
	by_runtime: parking_lot::Mutex<HashMap<u64, Arc<ContactLabelPlacement>>>,
}

impl ContactLabelPlacements {
	/// Acquire the connection's serial label layer.
	pub fn for_runtime(
		&self,
		runtime: u64,
		platform: Arc<dyn crate::platform::ContactsPlatform>,
		product: &crate::platform::ProductContext,
	) -> Arc<ContactLabelPlacement> {
		self.by_runtime
			.lock()
			.entry(runtime)
			.or_insert_with(|| {
				Arc::new(ContactLabelPlacement {
					platform,
					product: product.clone(),
					surface: Default::default(),
					closed: AtomicBool::new(false),
				})
			})
			.clone()
	}

	/// Prevent late draws and clear a connection's labels during teardown.
	pub fn release(&self, runtime: u64, spawner: &crate::subscription::Spawner) {
		let Some(placement) = self.by_runtime.lock().remove(&runtime) else {
			return;
		};
		placement.closed.store(true, Ordering::Release);
		spawner(Box::pin(async move { placement.clear(None).await }));
	}

	/// Clear labels from the preceding wallet session, without erasing newer draws.
	pub fn session_changed(&self, generation: u64, spawner: &crate::subscription::Spawner) {
		let placements: Vec<_> = self.by_runtime.lock().values().cloned().collect();
		if placements.is_empty() {
			return;
		}
		spawner(Box::pin(async move {
			for placement in placements {
				placement.clear(Some(generation)).await;
			}
		}));
	}
}

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
mod tests;
