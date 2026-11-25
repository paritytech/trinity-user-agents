//! The light client of a stack.
//!
//! `truapi-provider` checks and runs the light specs; this module gets them to
//! it. A spec comes from the ones this build ships with its built-in stacks,
//! from the copy the CLI kept the last time it passed the check, or from the
//! URL the stack's descriptor names. Start at
//! [`StackLightClient::new`].

use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	time::{Duration, Instant, SystemTime},
};

use trinity_stack::Descriptor;
use truapi::platform::{ChainProvider, JsonRpcConnection, ProviderError};
use truapi_provider::{EmbeddedChainProvider, LightSpecSource, LightStack};

use crate::{
	network::{ChainEndpoint, bundled_light_spec},
	platform::{StorageError, atomic_write},
	stack_descriptor::{FETCH_TIMEOUT, FetchError, Roles, fetch},
};

/// How long a kept spec outlives its last use. A relay's spec is replaced
/// whenever its operator publishes a fresher checkpoint, so old ones pile up.
const KEPT_SPEC_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// How long a spec that could not be fetched is reported unreachable without
/// fetching it again, so the chains waiting on it, as every parachain waits on
/// its relay's, do not each wait out another fetch.
const UNREACHABLE_SPEC_RETRY: Duration = Duration::from_secs(60);

/// One stack's chains on the embedded light client, kept running between
/// uses. It stores no database, so every process syncs from the specs'
/// checkpoints.
pub struct StackLightClient {
	provider: EmbeddedChainProvider,
	/// Checked specs by the `sha256` their descriptor names, so a run does not
	/// depend on the server a spec came from.
	spec_directory: PathBuf,
	/// Each light spec the descriptor names, by its chain's genesis hash, with
	/// its last failed fetch, locked while the spec is loaded. Connects that
	/// need the same spec share one fetch and its outcome, and a spec that
	/// cannot be fetched holds up no other chain.
	named: HashMap<[u8; 32], (LightSpecSource, tokio::sync::Mutex<Option<FailedFetch>>)>,
}

/// A fetch of a light spec that failed.
enum FailedFetch {
	/// The spec could not be fetched at `at`.
	Unreachable { at: Instant, cause: String },
	/// The spec is larger than the CLI reads, which another fetch in this run
	/// would not change.
	TooLarge,
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
	/// The spec is larger than the CLI reads, so fetching it again gets
	/// nowhere. A connect reports it as a refusal, not a transport failure.
	#[display("the light spec of {role} at {url} is {}", FetchError::TooLarge)]
	TooLarge {
		/// The chain's role in the descriptor.
		role: String,
		/// Where the spec is published.
		url: String,
	},
	/// The spec is not the one the descriptor names.
	#[display("{_0}")]
	Wrong(ProviderError),
}

impl StackLightClient {
	/// A light client for `descriptor`'s stack routed as `routed`, keeping
	/// checked specs in `spec_directory`. Fails as `LightStack::new` does, and
	/// when the descriptor names no light spec for a routed chain or its relay.
	pub fn new(
		descriptor: &Descriptor,
		routed: &Roles<ChainEndpoint>,
		spec_directory: PathBuf,
	) -> anyhow::Result<Self> {
		let stack = LightStack::new(descriptor.id.clone(), descriptor)?;
		let named = stack
			.light_specs()
			.into_iter()
			.map(|named| (named.genesis_hash, (named, tokio::sync::Mutex::new(None))))
			.collect();
		let light = Self {
			provider: EmbeddedChainProvider::builder().stack(stack).keep_chains_running().build(),
			spec_directory,
			named,
		};
		let run = light.chains(routed);
		for chain in &descriptor.chains {
			let genesis = chain.genesis();
			anyhow::ensure!(
				!run.contains(&genesis) || light.named.contains_key(&genesis),
				"--chain-backend light-client cannot run {}: its descriptor names no lightSpec for \
				 {}; use --chain-backend rpc",
				descriptor.id,
				chain.role
			);
		}
		Ok(light)
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
				Err(too_large @ MissingSpec::TooLarge { .. }) => {
					return Err(ProviderError::Host { reason: too_large.to_string() });
				},
				Err(unreachable @ MissingSpec::Unreachable { .. }) => {
					return Err(ProviderError::Transport { reason: unreachable.to_string() });
				},
			}
		}
		self.provider.connect(genesis_hash).await
	}

	/// Hands the provider the light spec of the chain with `genesis_hash`,
	/// once. The spec this build ships, then the kept copy, is used while it
	/// passes the check. Otherwise the spec is fetched, checked and kept. A
	/// spec that could not be fetched is reported unreachable without another
	/// fetch for [`UNREACHABLE_SPEC_RETRY`], and one too large to read is
	/// reported so for the rest of the run.
	pub async fn load_spec(&self, genesis_hash: [u8; 32]) -> Result<(), MissingSpec> {
		let (LightSpecSource { role, url, sha256, .. }, last_failure) = self
			.named
			.get(&genesis_hash)
			.ok_or(MissingSpec::Wrong(ProviderError::UnknownGenesis { genesis: genesis_hash }))?;
		let mut last_failure = last_failure.lock().await;
		let bundled = bundled_light_spec(sha256);
		if self.provider.has_light_spec(genesis_hash) ||
			bundled.is_some_and(|spec| {
				self.provider.add_light_spec(genesis_hash, spec.as_bytes()).is_ok()
			}) {
			return Ok(());
		}
		let unreachable = |cause: &str| MissingSpec::Unreachable {
			role: role.clone(),
			url: url.clone(),
			cause: cause.to_owned(),
		};
		match &*last_failure {
			Some(FailedFetch::TooLarge) => {
				return Err(MissingSpec::TooLarge { role: role.clone(), url: url.clone() });
			},
			Some(FailedFetch::Unreachable { at, cause })
				if at.elapsed() < UNREACHABLE_SPEC_RETRY =>
			{
				return Err(unreachable(cause));
			},
			_ => {},
		}
		let kept = self.spec_directory.join(format!("{sha256}.json"));

		let read = kept.clone();
		let kept_bytes = tokio::task::spawn_blocking(move || std::fs::read(read)).await;
		if let Ok(Ok(bytes)) = kept_bytes &&
			self.provider.add_light_spec(genesis_hash, &bytes).is_ok()
		{
			if let Ok(Err(error)) = tokio::task::spawn_blocking(move || mark_used(&kept)).await {
				tracing::debug!("cannot mark the kept light spec of {role} as used: {error}");
			}
			return Ok(());
		}

		let bytes = match fetch(url, FETCH_TIMEOUT).await {
			Ok(bytes) => bytes,
			Err(FetchError::TooLarge) => {
				*last_failure = Some(FailedFetch::TooLarge);
				return Err(MissingSpec::TooLarge { role: role.clone(), url: url.clone() });
			},
			Err(cause) => {
				let cause = cause.to_string();
				let error = unreachable(&cause);
				*last_failure = Some(FailedFetch::Unreachable { at: Instant::now(), cause });
				return Err(error);
			},
		};
		self.provider.add_light_spec(genesis_hash, &bytes).map_err(MissingSpec::Wrong)?;
		let kept_at = kept.display().to_string();
		let written = tokio::task::spawn_blocking(move || {
			keep(&kept, &bytes).map_err(|error| error.to_string())
		})
		.await
		.map_err(|error| error.to_string());
		if let Err(error) | Ok(Err(error)) = written {
			tracing::warn!(
				"cannot keep the light spec of {role} at {kept_at}, so the next run fetches it \
				 again: {error}"
			);
		}
		Ok(())
	}

	/// Forgets every spec that could not be fetched, so the next connect to
	/// its chain fetches it again rather than reporting the failure for
	/// [`UNREACHABLE_SPEC_RETRY`]. The start check calls it once it has
	/// logged them, since a command connects well within that time.
	pub async fn forget_unreachable(&self) {
		for (_, last_failure) in self.named.values() {
			let mut last_failure = last_failure.lock().await;
			if matches!(*last_failure, Some(FailedFetch::Unreachable { .. })) {
				*last_failure = None;
			}
		}
	}
}

/// Marks the kept spec at `path` as used now, since [`remove_expired`] counts
/// from a spec's last use.
fn mark_used(path: &Path) -> std::io::Result<()> {
	std::fs::File::options().write(true).open(path)?.set_modified(SystemTime::now())
}

/// Keeps `bytes` at `path`, replacing it in one step so a reader never sees a
/// spec cut short, and removes the specs beside it that went unused for
/// [`KEPT_SPEC_LIFETIME`].
fn keep(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
	atomic_write(path, bytes)?;
	if let Some(directory) = path.parent() {
		remove_expired(directory);
	}
	Ok(())
}

/// Removes the files of `directory` last used more than
/// [`KEPT_SPEC_LIFETIME`] ago. One that cannot be read or removed is left for
/// the next time.
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
