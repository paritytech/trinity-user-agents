//! The host's genesis to pipeline map, read once and kept.

use std::{collections::HashMap, sync::Arc};

use futures::lock::Mutex;
use subxt::utils::H256;

use super::ExtrinsicBuildError;
use crate::platform::ExtrinsicVersionHost;

/// The pipeline the host builds each V5 chain with. Read from the host on
/// first use and kept for the life of the service. A failed read is not
/// kept, so the next build asks again.
pub struct VersionMapCache {
	host: Arc<dyn ExtrinsicVersionHost>,
	versions: Mutex<Option<Arc<HashMap<H256, u8>>>>,
}

impl VersionMapCache {
	/// A cache that reads from `host` when first asked.
	pub fn new(host: Arc<dyn ExtrinsicVersionHost>) -> Self {
		Self { host, versions: Mutex::new(None) }
	}

	/// The pipeline the host builds `genesis` with, or `None` when the host
	/// builds it as V4.
	pub async fn extension_version(
		&self,
		genesis: H256,
	) -> Result<Option<u8>, ExtrinsicBuildError> {
		let mut versions = self.versions.lock().await;
		if versions.is_none() {
			*versions = Some(Arc::new(self.read_host_map().await?));
		}
		Ok(versions.as_ref().and_then(|map| map.get(&genesis).copied()))
	}

	async fn read_host_map(&self) -> Result<HashMap<H256, u8>, ExtrinsicBuildError> {
		let map = self
			.host
			.transaction_extension_versions()
			.await
			.map_err(|error| ExtrinsicBuildError::HostVersions(error.reason))?;
		Ok(map
			.chains
			.into_iter()
			.map(|chain| (H256(chain.genesis_hash), chain.extension_version))
			.collect())
	}
}
