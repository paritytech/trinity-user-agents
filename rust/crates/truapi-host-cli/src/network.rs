use anyhow::{Result, ensure};
use clap::ValueEnum;
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

/// Networks the headless hosts know by name, each a test network this build
/// ships a signed stack copy of. The CLI account store saves BIP-39 mnemonics
/// in plaintext (`accounts.rs`), which is only acceptable because every
/// network it serves has disposable test identities.
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
	/// PCF's devnet on Paseo, on the `dot` TLD, read from the descriptor PCF
	/// publishes.
	#[value(name = "pcf-devnet")]
	PcfDevnet,
}

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
	/// Identity backend base URL including `/api/v1`, from the stack's `dub`
	/// service. `None` on a stack without one.
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
	/// The identity backend base URL, or an error when the stack has no `dub`
	/// service.
	pub fn identity_backend(&self) -> anyhow::Result<&str> {
		self.identity_backend_base.as_deref().ok_or_else(|| {
			anyhow::anyhow!(
				"the {} stack has no identity backend: it lists no dub service",
				self.id
			)
		})
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
