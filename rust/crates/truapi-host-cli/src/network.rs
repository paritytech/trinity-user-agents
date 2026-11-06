use std::time::Duration;

use clap::ValueEnum;
use trinity_stack::Descriptor;
use truapi::{
	latest::ChainIdentifier,
	platform::{ChainExtensionVersion, ExtensionVersionMap, HostChainEntry, HostChainSet},
};

use crate::{chain::ChainBackend, stack_descriptor::Roles};

mod built_in;
mod checks;
mod polkadot_fork;
mod source;
mod stack;
#[cfg(test)]
mod testing;

pub use built_in::ensure_built_in_runnable;

/// Networks the headless hosts know by name, each a test network this build
/// ships a signed stack copy of. The CLI account store saves BIP-39 mnemonics
/// in plaintext (`accounts.rs`), which is acceptable because every built-in
/// network has disposable test identities; a `--stack` is assumed to be a test
/// network too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
	/// Paseo Next v2 test network (default).
	#[value(name = "paseo-next-v2")]
	#[default]
	PaseoNextV2,
	/// Previewnet test network.
	#[value(name = "previewnet")]
	Previewnet,
	/// PCF's devnet on Paseo, on the `dot` TLD.
	#[value(name = "pcf-devnet")]
	PcfDevnet,
}

/// Env var supplying `--stack` when the flag is absent; a blank value is unset.
pub const STACK_ENV: &str = "HOST_CLI_STACK";

/// How long a chain check before a command runs waits for a node: an
/// endpoint's genesis probe, and each side of the Polkadot fork comparison.
pub const CHAIN_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

/// `check`'s outcome, or an error once [`CHAIN_CHECK_TIMEOUT`] passes without
/// one.
pub async fn within_check_timeout<T>(
	check: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
	tokio::time::timeout(CHAIN_CHECK_TIMEOUT, check).await.unwrap_or_else(|_| {
		Err(anyhow::anyhow!("no answer within {}s", CHAIN_CHECK_TIMEOUT.as_secs()))
	})
}

/// The namespace no stack runs under, since it may contain sessions of real
/// Polkadot identities.
const RETIRED_POLKADOT_NAMESPACE: &str = "polkadot";

/// The product namespace of the stack `id`: `id` without a trailing `-fork`,
/// except that the `polkadot-fork` namespace is its full `id` rather than the
/// `polkadot` one.
fn namespace_of(id: &str) -> &str {
	id.strip_suffix("-fork")
		.filter(|source| *source != RETIRED_POLKADOT_NAMESPACE)
		.unwrap_or(id)
}

/// Where a command's network comes from: `--chain-backend`, `--network` and
/// `--stack`.
#[derive(Clone)]
pub struct NetworkSource {
	/// How the command connects to the chains.
	pub chain_backend: ChainBackend,
	/// paseo-next-v2 when neither this nor `stack` is set.
	pub network: Option<Network>,
	/// The stack to run against instead of a built-in network.
	pub stack: Option<StackArg>,
}

/// A `--stack` value and where it came from, for error messages.
#[derive(Clone)]
pub struct StackArg {
	/// A descriptor URL, a stack link or a local descriptor file.
	pub value: String,
	/// `--stack` or [`STACK_ENV`].
	pub source: &'static str,
}

/// The stack a command runs against, connected to over `--chain-backend`.
/// Built only by [`Stack::new`], which decodes every field the accessors
/// read, so none of them fails.
#[derive(Debug)]
pub struct Stack {
	descriptor: Descriptor,
	chain_backend: ChainBackend,
	chains: Roles<ChainEndpoint>,
	identity_backend: String,
	pop_controller: [u8; 20],
}

/// A chain's genesis hash and the WebSocket RPC URL to connect to it at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainEndpoint {
	pub genesis: [u8; 32],
	pub ws: String,
}

impl Stack {
	/// The product namespace: what `HostChainSet::network` reports and where
	/// the CLI stores this network's state. The descriptor's `id` without a
	/// trailing `-fork`, so a fork shares its source's state, except that the
	/// `polkadot-fork` namespace is its full `id`.
	pub fn namespace(&self) -> &str {
		namespace_of(&self.descriptor.id)
	}

	/// The network's dotNS TLD without the dot, as its runtimes report it in
	/// `NetworkSuffix.NetworkSuffix`. Every reserved RFC-0022 identity the CLI
	/// derives ends in it (`uid.<tld>`, `peopl.<tld>`), so a person the CLI
	/// creates here is the same person a phone derives from that seed on this
	/// network.
	pub fn tld(&self) -> &str {
		&self.descriptor.tld
	}

	/// Each role's genesis hash: both the role's identity in
	/// [`Stack::host_chain_set`] and its routing key. Asset Hub's is also the
	/// genesis the CLI's Asset Hub extrinsics sign in `CheckGenesis`.
	pub fn genesis(&self) -> Roles<[u8; 32]> {
		let Roles { people, asset_hub, bulletin } = &self.chains;
		Roles { people: people.genesis, asset_hub: asset_hub.genesis, bulletin: bulletin.genesis }
	}

	/// Each role's endpoint: its genesis hash and first RPC URL.
	pub fn chains(&self) -> &Roles<ChainEndpoint> {
		&self.chains
	}

	/// Identity backend base URL including `/api/v1`, from the stack's `dub`
	/// service.
	pub fn identity_backend(&self) -> &str {
		&self.identity_backend
	}

	/// The `DotnsPopController` address, the stack's `dotns.popController`.
	pub fn pop_controller(&self) -> [u8; 20] {
		self.pop_controller
	}

	/// How this run reaches the chains, from `--chain-backend`.
	pub fn chain_backend(&self) -> ChainBackend {
		self.chain_backend
	}

	/// The chain set this network serves, for `Features::supported_chains`.
	pub fn host_chain_set(&self) -> HostChainSet {
		let genesis = self.genesis();
		HostChainSet {
			network: self.namespace().to_owned(),
			chains: vec![
				HostChainEntry {
					identifier: ChainIdentifier::People,
					genesis_hash: genesis.people,
				},
				HostChainEntry {
					identifier: ChainIdentifier::Bulletin,
					genesis_hash: genesis.bulletin,
				},
				HostChainEntry {
					identifier: ChainIdentifier::AssetHub,
					genesis_hash: genesis.asset_hub,
				},
			],
		}
	}

	/// The chains this network builds as V5 general transactions: People and
	/// Asset Hub, both on pipeline 1, the one that declares their authorizers.
	/// The mobile apps read the same from remote config.
	pub fn extension_versions(&self) -> ExtensionVersionMap {
		let genesis = self.genesis();
		ExtensionVersionMap {
			chains: vec![
				ChainExtensionVersion { genesis_hash: genesis.people, extension_version: 1 },
				ChainExtensionVersion { genesis_hash: genesis.asset_hub, extension_version: 1 },
			],
		}
	}
}

#[cfg(test)]
mod tests;
