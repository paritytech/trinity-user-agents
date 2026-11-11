//! The light client of a stack.
//!
//! `truapi-provider` checks and runs the light specs; this module gets them to
//! it. A spec comes from the copy the CLI kept the last time it passed the
//! check, or from the URL the stack's descriptor names. Start at
//! [`StackLightClient::new`].

use std::{
	path::{Path, PathBuf},
	time::{Duration, SystemTime},
};

use trinity_stack::Descriptor;
use truapi::platform::{ChainProvider, JsonRpcConnection, ProviderError};
use truapi_provider::{EmbeddedChainProvider, LightSpecSource, LightStack};

use crate::{
	network::ChainEndpoint,
	platform::{StorageError, atomic_write},
	stack_descriptor::{FETCH_TIMEOUT, Roles, fetch},
};

/// How long a kept spec outlives its last write. A relay's spec is replaced
/// whenever its operator publishes a fresher checkpoint, so old ones pile up.
const KEPT_SPEC_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// One stack's chains on the embedded light client, kept running between
/// uses. It stores no database, so every process syncs from the specs'
/// checkpoints.
pub struct StackLightClient {
	provider: EmbeddedChainProvider,
	/// Checked specs by the `sha256` their descriptor names, so a run does not
	/// depend on the server a spec came from.
	spec_directory: PathBuf,
	/// Held while a spec is fetched, so connects that need the same spec
	/// fetch it once.
	fetching: tokio::sync::Mutex<()>,
}

impl core::fmt::Debug for StackLightClient {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("StackLightClient")
			.field("spec_directory", &self.spec_directory)
			.finish_non_exhaustive()
	}
}

/// Why a chain's light spec is not in the provider.
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum MissingSpec {
	/// The spec could not be fetched. The chain is unavailable, not wrong.
	#[display("the light spec of {role} at {url} is unreachable: {cause}")]
	Unreachable {
		/// The chain's role in the descriptor.
		role: String,
		/// Where the spec is published.
		url: String,
		/// Why the fetch failed.
		cause: String,
	},
	/// The spec is not the one the descriptor names.
	#[display("{_0}")]
	Wrong(ProviderError),
}

impl StackLightClient {
	/// A light client for the stack `stack_id` names, keeping checked specs in
	/// `spec_directory`. Fails as `LightStack::new` does.
	pub fn new(
		stack_id: &str,
		descriptor: &Descriptor,
		spec_directory: PathBuf,
	) -> Result<Self, ProviderError> {
		let stack = LightStack::new(stack_id, descriptor)?;
		Ok(Self {
			provider: EmbeddedChainProvider::builder().stack(stack).keep_chains_running().build(),
			spec_directory,
			fetching: tokio::sync::Mutex::new(()),
		})
	}

	/// The provider running the stack's chains.
	pub fn provider(&self) -> &EmbeddedChainProvider {
		&self.provider
	}

	/// The chains this client runs for a stack routed as `routed`: the relay
	/// they sync through, then People, Asset Hub and Bulletin.
	pub fn chains(&self, routed: &Roles<ChainEndpoint>) -> Vec<[u8; 32]> {
		let relay = self.provider.relay_of(routed.people.genesis);
		relay.into_iter().chain(routed.iter().map(|chain| chain.genesis)).collect()
	}

	/// Connects to the stack's chain with `genesis_hash`, getting its spec and
	/// its relay's first when the provider has neither. Fails as the provider
	/// does, and when a spec is wrong or cannot be fetched.
	pub async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		let relay = self.provider.relay_of(genesis_hash);
		for genesis in relay.into_iter().chain([genesis_hash]) {
			match self.load_spec(genesis).await {
				Ok(()) => {},
				Err(MissingSpec::Wrong(error)) => return Err(error),
				Err(unreachable @ MissingSpec::Unreachable { .. }) => {
					return Err(ProviderError::Transport { reason: unreachable.to_string() });
				},
			}
		}
		self.provider.connect(genesis_hash).await
	}

	/// Hands the provider the light spec of the chain with `genesis_hash`,
	/// once. The kept copy is used while it passes the check. Otherwise the
	/// spec is fetched, checked and kept.
	pub async fn load_spec(&self, genesis_hash: [u8; 32]) -> Result<(), MissingSpec> {
		let _fetching = self.fetching.lock().await;
		if self.provider.has_light_spec(genesis_hash) {
			return Ok(());
		}
		let LightSpecSource { role, url, sha256, .. } = self
			.provider
			.light_specs()
			.into_iter()
			.find(|named| named.genesis_hash == genesis_hash)
			.ok_or(MissingSpec::Wrong(ProviderError::UnknownGenesis { genesis: genesis_hash }))?;
		let kept = self.spec_directory.join(format!("{sha256}.json"));

		let read = kept.clone();
		let kept_bytes = tokio::task::spawn_blocking(move || std::fs::read(read)).await;
		if let Ok(Ok(bytes)) = kept_bytes &&
			self.provider.add_light_spec(genesis_hash, &bytes).is_ok()
		{
			return Ok(());
		}

		let bytes = match fetch(&url, FETCH_TIMEOUT).await {
			Ok(bytes) => bytes,
			Err(cause) => {
				return Err(MissingSpec::Unreachable { role, url, cause: cause.to_string() });
			},
		};
		self.provider.add_light_spec(genesis_hash, &bytes).map_err(MissingSpec::Wrong)?;
		let kept_at = kept.display().to_string();
		match tokio::task::spawn_blocking(move || keep(&kept, &bytes)).await {
			Ok(Ok(())) => {},
			Ok(Err(error)) => tracing::debug!("cannot keep the light spec at {kept_at}: {error}"),
			Err(error) => tracing::debug!("cannot keep the light spec at {kept_at}: {error}"),
		}
		Ok(())
	}
}

/// Keeps `bytes` at `path`, replacing it in one step so a reader never sees a
/// spec cut short, and removes the specs beside it that are older than
/// [`KEPT_SPEC_LIFETIME`].
fn keep(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
	atomic_write(path, bytes)?;
	if let Some(directory) = path.parent() {
		remove_expired(directory);
	}
	Ok(())
}

/// Removes the files of `directory` written more than [`KEPT_SPEC_LIFETIME`]
/// ago. One that cannot be read or removed is left for the next time.
fn remove_expired(directory: &Path) {
	let expired = |entry: std::fs::DirEntry| {
		let written = entry.metadata().and_then(|metadata| metadata.modified()).ok()?;
		let age = SystemTime::now().duration_since(written).ok()?;
		(age > KEPT_SPEC_LIFETIME).then(|| entry.path())
	};
	let Ok(entries) = std::fs::read_dir(directory) else {
		return;
	};
	for stale in entries.flatten().filter_map(expired) {
		let _ = std::fs::remove_file(stale);
	}
}

#[cfg(test)]
mod tests;
