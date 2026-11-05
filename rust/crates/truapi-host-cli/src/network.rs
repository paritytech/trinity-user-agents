use std::time::Duration;

use anyhow::{Result, anyhow};
use clap::ValueEnum;
use truapi::{
	latest::ChainIdentifier,
	platform::{ChainExtensionVersion, ExtensionVersionMap, HostChainEntry, HostChainSet},
};

use self::{
	checks::{ChainChecks, plan_chain_checks, refuse_light_client},
	overrides::{
		ChainOverrides, apply_backend_override, apply_chain_overrides, ensure_sources_compatible,
	},
	resolve::{BUILT_IN_REFRESH_TIMEOUT, BuiltInCopy},
};
use crate::{
	chain::ChainBackend,
	stack_descriptor::{FETCH_TIMEOUT, Roles, load_descriptor},
};

mod checks;
#[cfg(test)]
mod fixtures;
pub mod local_network;
mod overrides;
mod polkadot;
mod resolve;
mod stack;

/// Networks the headless hosts know by name.
///
/// The CLI account store saves BIP-39 mnemonics in plaintext (`accounts.rs`),
/// which is only acceptable for disposable test identities. A network that sets
/// [`NetworkConfig::disposable_identities`] may use that store; any other
/// network only signs with a mnemonic supplied per process, never writes one,
/// and never approves a confirmation without a prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Network {
	/// Built-in stack, read from the descriptor its operator publishes.
	#[value(name = "paseo-next-v2")]
	#[default]
	PaseoNextV2,
	/// Built-in stack, read from the descriptor its operator publishes. Its id is
	/// the id the playground and dotli already use for this network
	/// (`VITE_NETWORKS=paseo-next-v2,previewnet`), so `HostChainSet::network`
	/// reads the same string a product sees everywhere else.
	#[value(name = "previewnet")]
	Previewnet,
	/// Polkadot mainnet (People, Bulletin, Asset Hub). Identities are real:
	/// pass the signer with --mnemonic; no stored accounts, no auto-accept
	#[value(name = "polkadot")]
	Polkadot,
}

/// Env var overriding the identity backend base URL for every command. The URL
/// includes `/api/v1`, for instance a local backend at
/// `http://localhost:8080/api/v1`.
pub const IDENTITY_BACKEND_BASE_ENV: &str = "HOST_CLI_IDENTITY_BACKEND_BASE";

/// Env var supplying `--stack` when the flag is absent; a blank value is unset.
pub const STACK_ENV: &str = "HOST_CLI_STACK";

/// Env var overriding the People chain's WebSocket URL, for a fork of the
/// resolved network with the same genesis hash.
pub const PEOPLE_WS_ENV: &str = "HOST_CLI_PEOPLE_WS";
/// Env var overriding the Asset Hub chain's WebSocket URL, for a fork of the
/// resolved network with the same genesis hash.
pub const ASSET_HUB_WS_ENV: &str = "HOST_CLI_ASSET_HUB_WS";
/// Env var overriding the Bulletin chain's WebSocket URL, for a fork of the
/// resolved network with the same genesis hash.
pub const BULLETIN_WS_ENV: &str = "HOST_CLI_BULLETIN_WS";

/// Where a command's network comes from: `--chain-backend`, `--network` and
/// `--stack`.
#[derive(Clone)]
pub struct NetworkSource {
	pub chain_backend: ChainBackend,
	/// paseo-next-v2 when neither this nor `stack` is set.
	pub network: Option<Network>,
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

impl NetworkSource {
	/// The network a command runs against: the built-in network or the stack,
	/// then `HOST_CLI_*_WS` or `HOST_CLI_LOCAL_NETWORK_CONFIG` on top.
	///
	/// Fails when the routing sources cannot be combined, when a `--stack`
	/// descriptor is unreachable, when the descriptor in use is expired or
	/// lacks a role, when a disposable-identity network carries Polkadot's
	/// genesis hashes, TLD or namespace, when the light client cannot serve the
	/// network, when an endpoint reports a genesis other than the one it is
	/// routed as, when an endpoint an override reroutes does not answer, and
	/// when People's `NetworkSuffix` differs from a stack's TLD. A built-in
	/// network whose published descriptor is unreachable runs on its bundled
	/// copy; when that copy fails a check, the descriptor is fetched once more
	/// and the checks repeat only when the published copy is then in use.
	pub async fn cli_config(&self) -> Result<NetworkConfig> {
		let (config, checks) = self.resolve_routes(BUILT_IN_REFRESH_TIMEOUT).await?;
		let Err(error) = checks.verify(&config).await? else { return Ok(config) };
		if checks.copy_in_use() != Some(BuiltInCopy::Unreachable) {
			return Err(error);
		}
		tracing::info!("{error}; fetching the published descriptor again");
		let (config, checks) = self.resolve_routes(FETCH_TIMEOUT).await?;
		if checks.copy_in_use() != Some(BuiltInCopy::Published) {
			return Err(error);
		}
		checks.verify(&config).await??;
		Ok(config)
	}

	/// [`Self::cli_config`] without contacting any chain; only a descriptor is
	/// fetched. An unreachable descriptor fails only for `--stack`.
	pub async fn resolve(&self) -> Result<NetworkConfig> {
		Ok(self.resolve_routes(BUILT_IN_REFRESH_TIMEOUT).await?.0)
	}

	/// The network and the [`ChainChecks`] to run against its chains. A
	/// built-in stack's published descriptor gets `fetch_timeout`.
	async fn resolve_routes(
		&self,
		fetch_timeout: Duration,
	) -> Result<(NetworkConfig, ChainChecks)> {
		let overrides = ChainOverrides::from_env()?;
		let override_names = overrides.env_names_in_use();
		let local_path = local_network::config_path();
		ensure_sources_compatible(
			self.stack.as_ref(),
			self.network,
			&override_names,
			local_path.is_some(),
			self.chain_backend,
		)?;
		let (base, built_in) = match (&self.stack, &local_path) {
			(Some(stack), _) => {
				let descriptor = load_descriptor(&stack.value).await;
				let base = descriptor
					.and_then(|descriptor| {
						stack::stack_network_config(&descriptor, self.chain_backend)
					})
					.map_err(|error| anyhow!("{}: {error}", stack.source))?;
				(base, None)
			},
			// A local copy replaces every route, so the bundled copy is enough.
			(None, Some(_)) => (self.network.unwrap_or_default().config(self.chain_backend), None),
			(None, None) => {
				resolve::resolve_built_in(
					self.network.unwrap_or_default(),
					fetch_timeout,
					self.chain_backend,
				)
				.await?
			},
		};
		let mut config = apply_chain_overrides(
			apply_backend_override(base.clone(), std::env::var(IDENTITY_BACKEND_BASE_ENV).ok()),
			overrides,
		);
		if let Some(path) = &local_path {
			config = local_network::resolve(config, path)?;
		}
		refuse_light_client(&config)?;
		polkadot::ensure_no_real_identity_values(&config)?;
		let checks = plan_chain_checks(&config, &base, local_path.is_some(), built_in);
		Ok((config, checks))
	}
}

impl Network {
	/// The product namespace this network runs under.
	pub fn id(self) -> &'static str {
		match self {
			Self::PaseoNextV2 => "paseo-next-v2",
			Self::Previewnet => "previewnet",
			Self::Polkadot => polkadot::ID,
		}
	}

	/// The values compiled into this build, reached over `chain_backend`,
	/// without fetching anything: a built-in stack's bundled copy, or
	/// Polkadot's pinned values.
	pub fn config(self, chain_backend: ChainBackend) -> NetworkConfig {
		resolve::bundled_config(self, chain_backend)
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

/// A chain's genesis hash and the WebSocket RPC URL it is reached at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainEndpoint {
	pub genesis: [u8; 32],
	pub ws: String,
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
