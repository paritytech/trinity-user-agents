use anyhow::{Result, ensure};
use clap::ValueEnum;
use truapi::{
	latest::ChainIdentifier,
	platform::{ChainExtensionVersion, ExtensionVersionMap, HostChainEntry, HostChainSet},
};

use crate::{chain::ChainBackend, stack_descriptor::Roles};

mod built_in;
mod checks;
pub mod local_network;
mod overrides;
mod reserved;
mod source;
mod stack;
#[cfg(test)]
mod testing;

/// Networks the headless hosts know by name, each a stack this build ships a
/// signed copy of.
///
/// The CLI account store saves BIP-39 mnemonics in plaintext (`accounts.rs`),
/// which is only acceptable for disposable test identities. A network that sets
/// [`NetworkConfig::disposable_identities`] may use that store; any other
/// network only signs with a mnemonic supplied per process, never writes one,
/// and never approves a confirmation without a prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
	/// Read from the descriptor its operator publishes.
	#[value(name = "paseo-next-v2")]
	#[default]
	PaseoNextV2,
	/// Read from the descriptor its operator publishes. Its id is the id the
	/// playground and dotli already use for this network
	/// (`VITE_NETWORKS=paseo-next-v2,previewnet`), so `HostChainSet::network`
	/// reads the same string a product sees everywhere else.
	#[value(name = "previewnet")]
	Previewnet,
	/// PCF's devnet on Paseo, read from the descriptor PCF publishes. Its `dot`
	/// TLD derives the same keys as Polkadot, so its identities are real: pass
	/// the signer with --mnemonic; no stored accounts, no auto-accept
	#[value(name = "pcf-devnet")]
	PcfDevnet,
}

/// Env var overriding the identity backend base URL for every command. The URL
/// includes `/api/v1`, for instance a local backend at
/// `http://localhost:8080/api/v1`.
pub const IDENTITY_BACKEND_BASE_ENV: &str = "HOST_CLI_IDENTITY_BACKEND_BASE";

/// Env var supplying `--stack` when the flag is absent; a blank value is unset.
pub const STACK_ENV: &str = "HOST_CLI_STACK";

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

impl Network {
	/// The product namespace this network runs under.
	pub fn id(self) -> &'static str {
		match self {
			Self::PaseoNextV2 => "paseo-next-v2",
			Self::Previewnet => "previewnet",
			Self::PcfDevnet => "pcf-devnet",
		}
	}
}

/// Resolved RPC/backend/genesis values for one network.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
	/// The product namespace: what `HostChainSet::network` reports and where
	/// the CLI stores this network's state.
	pub id: String,
	/// The network's dotNS TLD without the dot, as its runtimes report it in
	/// `NetworkSuffix.NetworkSuffix`. Every reserved RFC-0022 identity the CLI
	/// derives ends in it (`uid.<suffix>`, `peopl.<suffix>`), so a person the
	/// CLI creates here is the same person a phone derives from that seed on
	/// this network. `live_people_chain::network_suffix_matches_the_preset`
	/// holds it against the chain.
	pub network_suffix: &'static str,
	/// Identity backend base URL including `/api/v1`. `None` on a network with
	/// no published backend, where [`IDENTITY_BACKEND_BASE_ENV`] must supply it.
	pub identity_backend_base: Option<String>,
	/// Each role's endpoint. A genesis hash is both the role's identity in
	/// [`NetworkConfig::host_chain_set`] and its routing key; Asset Hub's is
	/// also the genesis the CLI's Asset Hub extrinsics sign in `CheckGenesis`.
	///
	/// Wrong here is worse than stale elsewhere: routing matches on the
	/// genesis, so a value the chain does not report sends that role's traffic
	/// to the People chain.
	pub chains: Roles<ChainEndpoint>,
	/// `DotnsPopController` to use when on-chain discovery finds none, for a
	/// network whose gateway dispatcher is not set yet. Discovery wins when it
	/// answers.
	pub dotns_pop_controller: Option<[u8; 20]>,
	/// Whether identities on this network are disposable test identities the
	/// plaintext account store may save and auto-accept may sign for. Only
	/// test networks set it.
	pub disposable_identities: bool,
	/// How this run reaches the chains, from `--chain-backend`.
	pub chain_backend: ChainBackend,
}

/// A chain's genesis hash and the WebSocket RPC URL to connect to it at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainEndpoint {
	pub genesis: [u8; 32],
	pub ws: String,
}

impl Roles<ChainEndpoint> {
	/// Fails when two roles share a genesis hash, since the host routes each
	/// role by its genesis.
	fn ensure_distinct_genesis(&self) -> Result<()> {
		let Self { people, asset_hub, bulletin } = self;
		ensure!(
			people.genesis != asset_hub.genesis &&
				people.genesis != bulletin.genesis &&
				asset_hub.genesis != bulletin.genesis,
			"People, Asset Hub and Bulletin need distinct genesis hashes: the host routes each \
			 role by its genesis"
		);
		Ok(())
	}
}

impl NetworkConfig {
	/// The identity backend base URL, or an error naming the override when the
	/// network has none.
	pub fn identity_backend(&self) -> anyhow::Result<&str> {
		self.identity_backend_base.as_deref().ok_or_else(|| {
			anyhow::anyhow!(
				"{} has no published identity backend; set {IDENTITY_BACKEND_BASE_ENV} to its \
				 base URL, including /api/v1",
				self.id
			)
		})
	}

	/// Refuses to write a mnemonic for this network to the plaintext account
	/// store unless its identities are disposable.
	pub fn ensure_disposable_identities(&self) -> anyhow::Result<()> {
		anyhow::ensure!(
			self.disposable_identities,
			"{} has real identities, which the plaintext account store must not save; \
			 pass the signer with --mnemonic or HOST_CLI_SIGNER_MNEMONIC",
			self.id
		);
		Ok(())
	}

	/// The chain set this network serves, for `Features::supported_chains`.
	pub fn host_chain_set(&self) -> HostChainSet {
		HostChainSet {
			network: self.id.clone(),
			chains: vec![
				HostChainEntry {
					identifier: ChainIdentifier::People,
					genesis_hash: self.chains.people.genesis,
				},
				HostChainEntry {
					identifier: ChainIdentifier::Bulletin,
					genesis_hash: self.chains.bulletin.genesis,
				},
				HostChainEntry {
					identifier: ChainIdentifier::AssetHub,
					genesis_hash: self.chains.asset_hub.genesis,
				},
			],
		}
	}

	/// The chains this network builds as V5 general transactions: People and
	/// Asset Hub, both on pipeline 1, the one that declares their authorizers.
	/// The mobile apps read the same from remote config.
	pub fn extension_versions(&self) -> ExtensionVersionMap {
		ExtensionVersionMap {
			chains: vec![
				ChainExtensionVersion {
					genesis_hash: self.chains.people.genesis,
					extension_version: 1,
				},
				ChainExtensionVersion {
					genesis_hash: self.chains.asset_hub.genesis,
					extension_version: 1,
				},
			],
		}
	}

	/// The endpoint of a served role; `None` for the relay chain, which no
	/// network routes.
	#[cfg(test)]
	pub fn endpoint_for_role(&self, role: ChainIdentifier) -> Option<&ChainEndpoint> {
		match role {
			ChainIdentifier::People => Some(&self.chains.people),
			ChainIdentifier::Bulletin => Some(&self.chains.bulletin),
			ChainIdentifier::AssetHub => Some(&self.chains.asset_hub),
			ChainIdentifier::Relay => None,
		}
	}
}

#[cfg(test)]
mod tests;
