//! Durable, wallet-scoped RFC-0024 ring-VRF registry snapshots.

use std::{
	collections::{HashMap, HashSet},
	sync::{Arc, Mutex},
};

use crate::platform::{CoreStorageKey, Platform, ProductId};
use parity_scale_codec::{Decode, Encode};
use truapi::latest::{ProductAccountId, RegisteredRingVrfKey, RingLocation, RingVrfKeyDisclosure};

use crate::host_internal::sso_messages::RingVrfError;

#[derive(Clone, PartialEq, Eq, Encode, Decode)]
struct SelectedProvider {
	ring: RingLocation,
	handle: ProductAccountId,
}

#[derive(Clone, Default, PartialEq, Eq, Encode, Decode)]
struct RegistrySnapshot {
	entries: Vec<RegisteredRingVrfKey>,
	/// Owners for which `entries` is a complete Account Holder snapshot.
	complete_owners: Vec<ProductId>,
	/// First registration wins until the user selects another provider.
	selected_providers: Vec<SelectedProvider>,
}

/// Shared durable repository used by both account-authority roles.
pub struct RingVrfRegistryStore {
	platform: Arc<dyn Platform>,
	cache: Mutex<HashMap<[u8; 32], RegistrySnapshot>>,
	storage_guard: futures::lock::Mutex<()>,
}

/// Prepared registry mutation bound to one root and storage lock.
pub struct RegistryUpdate<'a> {
	store: &'a RingVrfRegistryStore,
	root_public_key: [u8; 32],
	snapshot: RegistrySnapshot,
	_guard: futures::lock::MutexGuard<'a, ()>,
}

impl RegistryUpdate<'_> {
	/// Add a registration without replacing an existing key.
	pub fn register(
		&mut self,
		handle: ProductAccountId,
		ring: RingLocation,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		if let Some(entry) = self.snapshot.entries.iter_mut().find(|entry| entry.handle == handle) {
			if entry.public_key != Some(public_key) {
				return Err(invalid_registry("registered key handle has a conflicting public key"));
			}
			if !entry.rings.contains(&ring) {
				entry.rings.push(ring.clone());
			}
		} else {
			self.snapshot.entries.push(RegisteredRingVrfKey {
				handle: handle.clone(),
				rings: vec![ring.clone()],
				public_key: Some(public_key),
			});
		}
		if !self.snapshot.selected_providers.iter().any(|provider| provider.ring == ring) {
			self.snapshot.selected_providers.push(SelectedProvider { ring, handle });
		}
		Ok(())
	}

	/// Select a provider already registered for this ring.
	pub fn select_provider(
		&mut self,
		ring: RingLocation,
		handle: ProductAccountId,
	) -> Result<(), RingVrfError> {
		let registered = self
			.snapshot
			.entries
			.iter()
			.any(|entry| entry.handle == handle && entry.rings.contains(&ring));
		if !registered {
			return Err(RingVrfError::KeyNotInRing);
		}
		self.snapshot.selected_providers.retain(|provider| provider.ring != ring);
		self.snapshot.selected_providers.push(SelectedProvider { ring, handle });
		Ok(())
	}

	/// Merge a complete owner listing without discarding local registrations.
	pub fn reconcile_owner(
		&mut self,
		owner: &ProductId,
		entries: Vec<RegisteredRingVrfKey>,
	) -> Result<Vec<RegisteredRingVrfKey>, RingVrfError> {
		validate_authoritative_owner_entries(owner, &entries)?;
		let snapshot = &mut self.snapshot;
		let mut owner_entries = snapshot
			.entries
			.iter()
			.filter(|entry| entry.handle.dot_ns_identifier == owner.as_str())
			.cloned()
			.collect::<Vec<_>>();
		for entry in entries {
			if let Some(existing) =
				owner_entries.iter_mut().find(|existing| existing.handle == entry.handle)
			{
				if existing.public_key != entry.public_key {
					return Err(invalid_registry_listing(
						"owner snapshot conflicts with a locally registered public key",
					));
				}
				for ring in entry.rings {
					if !existing.rings.contains(&ring) {
						existing.rings.push(ring);
					}
				}
			} else {
				owner_entries.push(entry);
			}
		}
		snapshot.entries.retain(|entry| entry.handle.dot_ns_identifier != *owner);
		snapshot.entries.extend(owner_entries.iter().cloned());
		if !snapshot.complete_owners.contains(owner) {
			snapshot.complete_owners.push(owner.clone());
		}
		snapshot.selected_providers.retain(|provider| {
			snapshot.entries.iter().any(|entry| {
				entry.handle == provider.handle && entry.rings.contains(&provider.ring)
			})
		});
		for entry in &snapshot.entries {
			for ring in &entry.rings {
				if !snapshot.selected_providers.iter().any(|provider| provider.ring == *ring) {
					snapshot.selected_providers.push(SelectedProvider {
						ring: ring.clone(),
						handle: entry.handle.clone(),
					});
				}
			}
		}
		Ok(owner_entries)
	}

	/// Persist only to the root captured during preparation.
	async fn persist(self) -> Result<(), RingVrfError> {
		self.store.persist_under_guard(self.root_public_key, self.snapshot).await
	}
}

impl RingVrfRegistryStore {
	pub fn new(platform: Arc<dyn Platform>) -> Arc<Self> {
		Arc::new(Self {
			platform,
			cache: Mutex::new(HashMap::new()),
			storage_guard: futures::lock::Mutex::new(()),
		})
	}

	pub async fn entry(
		&self,
		root_public_key: [u8; 32],
		handle: &ProductAccountId,
	) -> Result<Option<RegisteredRingVrfKey>, RingVrfError> {
		Ok(self
			.snapshot(root_public_key)
			.await?
			.entries
			.into_iter()
			.find(|entry| entry.handle == *handle))
	}

	pub async fn complete_owner_entries(
		&self,
		root_public_key: [u8; 32],
		owner: &ProductId,
	) -> Result<Option<Vec<RegisteredRingVrfKey>>, RingVrfError> {
		let snapshot = self.snapshot(root_public_key).await?;
		if !snapshot.complete_owners.contains(owner) {
			return Ok(None);
		}
		Ok(Some(
			snapshot
				.entries
				.into_iter()
				.filter(|entry| entry.handle.dot_ns_identifier == owner.as_str())
				.collect(),
		))
	}

	pub async fn owner_entries(
		&self,
		root_public_key: [u8; 32],
		owner: &ProductId,
	) -> Result<Vec<RegisteredRingVrfKey>, RingVrfError> {
		Ok(self
			.snapshot(root_public_key)
			.await?
			.entries
			.into_iter()
			.filter(|entry| entry.handle.dot_ns_identifier == owner.as_str())
			.collect())
	}

	/// Persist a registration without replacing an existing key.
	pub async fn register(
		&self,
		root_public_key: [u8; 32],
		handle: ProductAccountId,
		ring: RingLocation,
		public_key: [u8; 32],
	) -> Result<(), RingVrfError> {
		let mut update = self.prepare_update(root_public_key).await?;
		update.register(handle, ring, public_key)?;
		update.persist().await
	}

	/// Load one root's registry while retaining storage serialization.
	async fn prepare_update(
		&self,
		root_public_key: [u8; 32],
	) -> Result<RegistryUpdate<'_>, RingVrfError> {
		let guard = self.storage_guard.lock().await;
		let snapshot = self.load_under_guard(root_public_key).await?;
		Ok(RegistryUpdate { store: self, root_public_key, snapshot, _guard: guard })
	}

	/// Reconcile one owner's complete response with locally registered keys.
	///
	/// RFC-0024 has no revocation operation, so a remote response cannot
	/// invalidate an entry already accepted by this host. This also prevents a
	/// list response created before a fire-and-forget registration mirror from
	/// removing that local registration.
	pub async fn reconcile_owner(
		&self,
		root_public_key: [u8; 32],
		owner: &ProductId,
		entries: Vec<RegisteredRingVrfKey>,
	) -> Result<Vec<RegisteredRingVrfKey>, RingVrfError> {
		let mut update = self.prepare_update(root_public_key).await?;
		let entries = update.reconcile_owner(owner, entries)?;
		update.persist().await?;
		Ok(entries)
	}

	pub async fn selected_provider(
		&self,
		root_public_key: [u8; 32],
		ring: &RingLocation,
	) -> Result<Option<ProductAccountId>, RingVrfError> {
		Ok(self
			.snapshot(root_public_key)
			.await?
			.selected_providers
			.into_iter()
			.find(|provider| provider.ring == *ring)
			.map(|provider| provider.handle))
	}

	pub async fn providers(
		&self,
		root_public_key: [u8; 32],
		ring: &RingLocation,
	) -> Result<Vec<ProductAccountId>, RingVrfError> {
		Ok(self
			.snapshot(root_public_key)
			.await?
			.entries
			.into_iter()
			.filter(|entry| entry.rings.contains(ring))
			.map(|entry| entry.handle)
			.collect())
	}

	/// Persist a user-selected provider after validating its registration.
	pub async fn select_provider(
		&self,
		root_public_key: [u8; 32],
		ring: RingLocation,
		handle: ProductAccountId,
	) -> Result<(), RingVrfError> {
		let mut update = self.prepare_update(root_public_key).await?;
		update.select_provider(ring, handle)?;
		update.persist().await
	}

	async fn snapshot(&self, root_public_key: [u8; 32]) -> Result<RegistrySnapshot, RingVrfError> {
		if let Some(snapshot) = self
			.cache
			.lock()
			.expect("ring-VRF registry cache mutex poisoned")
			.get(&root_public_key)
			.cloned()
		{
			return Ok(snapshot);
		}
		let _guard = self.storage_guard.lock().await;
		self.load_under_guard(root_public_key).await
	}

	async fn load_under_guard(
		&self,
		root_public_key: [u8; 32],
	) -> Result<RegistrySnapshot, RingVrfError> {
		if let Some(snapshot) = self
			.cache
			.lock()
			.expect("ring-VRF registry cache mutex poisoned")
			.get(&root_public_key)
			.cloned()
		{
			return Ok(snapshot);
		}
		let key = CoreStorageKey::RingVrfRegistry { root_public_key };
		let snapshot =
			match self.platform.read_core_storage(key.clone()).await.map_err(storage_error)? {
				Some(blob) => match decode_snapshot(&blob) {
					Ok(snapshot) => snapshot,
					Err(error) => {
						let _ = self.platform.clear_core_storage(key).await;
						return Err(error);
					},
				},
				None => RegistrySnapshot::default(),
			};
		self.cache
			.lock()
			.expect("ring-VRF registry cache mutex poisoned")
			.insert(root_public_key, snapshot.clone());
		Ok(snapshot)
	}

	async fn persist_under_guard(
		&self,
		root_public_key: [u8; 32],
		snapshot: RegistrySnapshot,
	) -> Result<(), RingVrfError> {
		validate_snapshot(&snapshot)?;
		if self
			.cache
			.lock()
			.expect("ring-VRF registry cache mutex poisoned")
			.get(&root_public_key) ==
			Some(&snapshot)
		{
			return Ok(());
		}
		self.platform
			.write_core_storage(
				CoreStorageKey::RingVrfRegistry { root_public_key },
				snapshot.encode(),
			)
			.await
			.map_err(storage_error)?;
		self.cache
			.lock()
			.expect("ring-VRF registry cache mutex poisoned")
			.insert(root_public_key, snapshot);
		Ok(())
	}
}

fn decode_snapshot(blob: &[u8]) -> Result<RegistrySnapshot, RingVrfError> {
	let mut input = blob;
	let snapshot = RegistrySnapshot::decode(&mut input).map_err(|error| RingVrfError::Unknown {
		reason: format!("invalid persisted ring-VRF registry: {error}"),
	})?;
	if !input.is_empty() {
		return Err(RingVrfError::Unknown {
			reason: "invalid persisted ring-VRF registry: trailing bytes".to_string(),
		});
	}
	validate_snapshot(&snapshot)?;
	Ok(snapshot)
}

fn validate_snapshot(snapshot: &RegistrySnapshot) -> Result<(), RingVrfError> {
	let mut handles = HashSet::new();
	for entry in &snapshot.entries {
		if entry.public_key.is_none() {
			return Err(invalid_registry("stored entry is missing its public key"));
		}
		if entry.rings.is_empty() {
			return Err(invalid_registry("stored entry has no declared rings"));
		}
		if !handles.insert(entry.handle.encode()) {
			return Err(invalid_registry("duplicate key handle"));
		}
		let mut rings = HashSet::new();
		if entry.rings.iter().any(|ring| !rings.insert(ring.encode())) {
			return Err(invalid_registry("duplicate declared ring"));
		}
	}
	let mut owners = HashSet::new();
	for owner in &snapshot.complete_owners {
		if !owners.insert(owner) {
			return Err(invalid_registry("duplicate complete owner"));
		}
	}
	let mut provider_rings = HashSet::new();
	for provider in &snapshot.selected_providers {
		if !provider_rings.insert(provider.ring.encode()) {
			return Err(invalid_registry("duplicate selected provider"));
		}
		if !snapshot
			.entries
			.iter()
			.any(|entry| entry.handle == provider.handle && entry.rings.contains(&provider.ring))
		{
			return Err(invalid_registry("selected provider is not registered for its ring"));
		}
	}
	Ok(())
}

/// Hide public keys from a listing that asked for anonymized entries.
pub fn apply_ring_vrf_disclosure(
	entries: &mut [RegisteredRingVrfKey],
	disclosure: RingVrfKeyDisclosure,
) {
	if disclosure == RingVrfKeyDisclosure::Anonymized {
		for entry in entries {
			entry.public_key = None;
		}
	}
}

pub fn validate_owner_listing(
	owner: &ProductId,
	entries: &[RegisteredRingVrfKey],
) -> Result<(), RingVrfError> {
	let mut handles = HashSet::new();
	for entry in entries {
		if entry.handle.dot_ns_identifier != owner.as_str() {
			return Err(invalid_registry_listing("owner listing contains a foreign key handle"));
		}
		if entry.rings.is_empty() {
			return Err(invalid_registry_listing("owner listing contains a key with no rings"));
		}
		if !handles.insert(entry.handle.encode()) {
			return Err(invalid_registry_listing("owner listing contains a duplicate key handle"));
		}
		let mut rings = HashSet::new();
		if entry.rings.iter().any(|ring| !rings.insert(ring.encode())) {
			return Err(invalid_registry_listing(
				"owner listing contains a duplicate declared ring",
			));
		}
	}
	Ok(())
}

fn validate_authoritative_owner_entries(
	owner: &ProductId,
	entries: &[RegisteredRingVrfKey],
) -> Result<(), RingVrfError> {
	validate_owner_listing(owner, entries)?;
	if entries.iter().any(|entry| entry.public_key.is_none()) {
		return Err(invalid_registry_listing(
			"authoritative owner snapshot contains a key without its public key",
		));
	}
	Ok(())
}

fn storage_error(error: truapi::v01::GenericError) -> RingVrfError {
	RingVrfError::Unknown { reason: format!("ring-VRF registry storage failed: {}", error.reason) }
}

fn invalid_registry(reason: impl Into<String>) -> RingVrfError {
	RingVrfError::Unknown {
		reason: format!("invalid persisted ring-VRF registry: {}", reason.into()),
	}
}

fn invalid_registry_listing(reason: impl Into<String>) -> RingVrfError {
	RingVrfError::Unknown {
		reason: format!("invalid ring-VRF registry listing: {}", reason.into()),
	}
}

#[cfg(test)]
mod tests;
