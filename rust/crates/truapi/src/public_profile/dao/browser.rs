//! Browser profile resolution has no persistent metadata cache.

use std::sync::Arc;

use super::{CachedRecord, ProfileRecord, PublicProfileError, PublicProfilesService};
use crate::Spawner;

/// Browser cache operations retain no state.
pub struct Cache;

impl Cache {
	/// Creates a cache that always misses.
	pub fn new(_spawner: Spawner) -> Self {
		Self
	}

	/// Browser profile lookups require no storage.
	pub fn check_storage(&self) -> Result<(), PublicProfileError> {
		Ok(())
	}
}

impl PublicProfilesService {
	/// Browser metadata is always resolved through dotNS.
	pub async fn stored_record(
		self: &Arc<Self>,
		_username: &str,
	) -> Result<Option<CachedRecord>, PublicProfileError> {
		Ok(None)
	}

	/// Browser profile records are not persisted.
	pub async fn store_record(
		self: &Arc<Self>,
		_username: &str,
		_record: ProfileRecord,
	) -> Result<(), PublicProfileError> {
		Ok(())
	}
}
