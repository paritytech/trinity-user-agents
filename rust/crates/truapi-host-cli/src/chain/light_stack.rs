//! The light client of a stack.
//!
//! `truapi-provider` checks and runs the light specs; this module gets them to
//! it. A spec comes from the copy the CLI kept the last time it passed the
//! check, or from the `lightSpec.url` the stack's descriptor names. Start at
//! [`StackLightClient::of`].

use std::{
	path::{Path, PathBuf},
	sync::{Arc, Mutex, PoisonError},
};

use trinity_stack::Descriptor;
use truapi::platform::{ChainProvider, JsonRpcConnection, ProviderError};
use truapi_provider::{EmbeddedChainProvider, LightStack};

use crate::{
	network::Stack,
	stack_descriptor::{FETCH_TIMEOUT, fetch},
};

/// Where checked light specs are kept, under the CLI's state directory.
const SPEC_DIRECTORY: &str = "light-specs";

/// One stack's chains on the embedded light client, kept running between
/// uses.
pub struct StackLightClient {
	provider: EmbeddedChainProvider,
	stack_id: String,
	descriptor: Descriptor,
	/// Checked specs by the `sha256` their descriptor names, so a run does not
	/// depend on the server a spec came from.
	spec_directory: PathBuf,
	/// The chains whose spec the provider holds.
	loaded: Mutex<Vec<[u8; 32]>>,
}

/// Why a chain's light spec is not in the provider.
#[derive(Debug)]
pub enum MissingSpec {
	/// The spec could not be fetched. The chain is unavailable, not wrong.
	Unreachable(String),
	/// The spec is not the one the descriptor names.
	Wrong(ProviderError),
}

impl StackLightClient {
	/// The process's light client for `stack`. Shared, because each
	/// `EmbeddedChainProvider` runs its own smoldot, and one per stack and
	/// descriptor: another descriptor of the same stack may name other chains
	/// or specs.
	pub fn of(stack: &Stack) -> Result<Arc<Self>, ProviderError> {
		static CLIENTS: Mutex<Vec<Arc<StackLightClient>>> = Mutex::new(Vec::new());

		let (stack_id, descriptor) = (stack.stack_id(), stack.descriptor());
		let mut clients = CLIENTS.lock().unwrap_or_else(PoisonError::into_inner);
		let running = clients
			.iter()
			.find(|client| client.stack_id == stack_id && client.descriptor == *descriptor);
		if let Some(client) = running {
			return Ok(Arc::clone(client));
		}
		let directory = crate::default_base_path().join(SPEC_DIRECTORY);
		let client = Arc::new(Self::new(&stack_id, descriptor, directory)?);
		clients.push(Arc::clone(&client));
		Ok(client)
	}

	/// A light client for the stack `stack_id` names, keeping checked specs in
	/// `spec_directory`. Fails when two chains of `descriptor` share a genesis
	/// hash.
	pub fn new(
		stack_id: &str,
		descriptor: &Descriptor,
		spec_directory: PathBuf,
	) -> Result<Self, ProviderError> {
		let stack = LightStack::new(stack_id, descriptor)?;
		Ok(Self {
			provider: EmbeddedChainProvider::builder().stack(stack).keep_chains_running().build(),
			stack_id: stack_id.to_owned(),
			descriptor: descriptor.clone(),
			spec_directory,
			loaded: Mutex::new(Vec::new()),
		})
	}

	/// The provider running the stack's chains.
	pub fn provider(&self) -> &EmbeddedChainProvider {
		&self.provider
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
				Err(MissingSpec::Unreachable(reason)) => {
					return Err(ProviderError::Transport { reason });
				},
			}
		}
		self.provider.connect(genesis_hash).await
	}

	/// Hands the provider the light spec of the chain with `genesis_hash`,
	/// once. The kept copy is used when it still passes the check, and the
	/// spec is fetched otherwise and kept.
	pub async fn load_spec(&self, genesis_hash: [u8; 32]) -> Result<(), MissingSpec> {
		if self.loaded().contains(&genesis_hash) {
			return Ok(());
		}
		let chain = self
			.descriptor
			.chains
			.iter()
			.find(|chain| chain.genesis() == genesis_hash)
			.ok_or(MissingSpec::Wrong(ProviderError::UnknownGenesis { genesis: genesis_hash }))?;
		let named = chain.light_spec.as_ref().ok_or_else(|| {
			MissingSpec::Wrong(ProviderError::LightSpec {
				role: chain.role.clone(),
				genesis: genesis_hash,
				reason: "the descriptor names no lightSpec".to_owned(),
			})
		})?;
		let kept = self.spec_directory.join(format!("{}.json", named.sha256));
		let from_kept = std::fs::read(&kept)
			.ok()
			.filter(|bytes| self.provider.add_light_spec(genesis_hash, bytes).is_ok());
		if from_kept.is_none() {
			let bytes = fetch(&named.url, FETCH_TIMEOUT).await.map_err(|cause| {
				MissingSpec::Unreachable(format!(
					"the light spec of {} at {} is unreachable: {cause}",
					chain.role, named.url
				))
			})?;
			self.provider.add_light_spec(genesis_hash, &bytes).map_err(MissingSpec::Wrong)?;
			if let Err(error) = keep(&kept, &bytes) {
				tracing::debug!("cannot keep the light spec at {}: {error}", kept.display());
			}
		}
		self.loaded().push(genesis_hash);
		Ok(())
	}

	fn loaded(&self) -> std::sync::MutexGuard<'_, Vec<[u8; 32]>> {
		self.loaded.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

/// Writes `bytes` at `path` through a file beside it, so a reader never sees a
/// spec cut short.
fn keep(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
	let directory = path.parent().unwrap_or(Path::new("."));
	std::fs::create_dir_all(directory)?;
	let mut file = tempfile::NamedTempFile::new_in(directory)?;
	std::io::Write::write_all(&mut file, bytes)?;
	file.persist(path)?;
	Ok(())
}

#[cfg(test)]
mod tests;
