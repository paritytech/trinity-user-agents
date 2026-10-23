//! [`ExtrinsicService`]: builds extrinsics born at a chain's finalized head.

use std::sync::Arc;

use subxt::utils::{AccountId32, H256};

use super::{
	ExtrinsicBuildError, ExtrinsicBuilder,
	builder::{RuntimeAtBirth, assemble, start_build},
	version_map::VersionMapCache,
};
use crate::{
	chain::{AccountNonces, HashAndNumber, MortalExtrinsic, WatchedStorage},
	chain_runtime::{ChainRuntime, HostChainProvider},
	platform::{ChainProvider, ExtrinsicVersionHost},
	subscription::Spawner,
};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

/// What [`ExtrinsicService`] reads from the chain and the host.
pub struct ExtrinsicServiceDeps {
	/// The runtime at the finalized head.
	pub storage: Arc<dyn WatchedStorage>,
	/// The signer's next nonce.
	pub nonces: Arc<dyn AccountNonces>,
	/// Which chains the host builds as V5.
	pub versions: Arc<dyn ExtrinsicVersionHost>,
}

/// Builds signed or authorized extrinsics born at a chain's finalized head.
/// It does not submit them.
pub struct ExtrinsicService {
	storage: Arc<dyn WatchedStorage>,
	nonces: Arc<dyn AccountNonces>,
	versions: VersionMapCache,
}

impl ExtrinsicService {
	/// A service over `deps`.
	pub fn new(deps: ExtrinsicServiceDeps) -> Self {
		let ExtrinsicServiceDeps { storage, nonces, versions } = deps;
		Self { storage, nonces, versions: VersionMapCache::new(versions) }
	}

	/// A service that reaches chains through `provider`, for programs that
	/// run without the core, such as the CLI.
	pub fn from_provider(
		provider: Arc<dyn ChainProvider>,
		spawner: Spawner,
		versions: Arc<dyn ExtrinsicVersionHost>,
	) -> Self {
		let chain = ChainRuntime::new(Arc::new(HostChainProvider::new(provider)), spawner);
		Self::over_chain(chain, versions)
	}

	/// A service that reads the chain through `chain`, sharing its pooled
	/// connections.
	pub fn over_chain(chain: ChainRuntime, versions: Arc<dyn ExtrinsicVersionHost>) -> Self {
		Self::new(ExtrinsicServiceDeps {
			storage: Arc::new(chain.clone()),
			nonces: Arc::new(chain),
			versions,
		})
	}

	/// Builds the extrinsic `build` describes on the chain `genesis`, mortal
	/// from the chain's finalized head. `genesis` is what the extrinsic
	/// signs, whatever the node reports.
	pub async fn build_mortal_extrinsic<'a>(
		&self,
		genesis: H256,
		build: impl FnOnce(&mut ExtrinsicBuilder<'a>),
	) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
		let runtime = self.runtime_at_finalized_head(genesis).await?;
		let mut builder = start_build(runtime);
		build(&mut builder);
		let host_extension_version = || self.versions.host_extension_version(genesis);
		let account_nonce = |account| self.read_account_nonce(genesis, account);
		assemble(builder, host_extension_version, account_nonce).await
	}

	async fn runtime_at_finalized_head(
		&self,
		genesis: H256,
	) -> Result<RuntimeAtBirth, ExtrinsicBuildError> {
		let client = self.storage.client_at_finalized_head(genesis).await?;
		Ok(RuntimeAtBirth {
			genesis,
			birth: HashAndNumber { hash: client.block_hash(), number: client.block_number() },
			spec_version: client.spec_version(),
			transaction_version: client.transaction_version(),
			metadata: client.metadata(),
		})
	}

	/// The next nonce of the signer `account`.
	async fn read_account_nonce(
		&self,
		genesis: H256,
		account: AccountId32,
	) -> Result<u64, ExtrinsicBuildError> {
		Ok(self.nonces.next_nonce(genesis, &account).await?)
	}
}
