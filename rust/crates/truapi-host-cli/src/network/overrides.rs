//! What the environment and the command line change on a resolved network:
//! `HOST_CLI_*_WS`, the identity backend override, and which routing sources
//! may be combined.

use anyhow::{Result, anyhow, ensure};

use super::{
	ASSET_HUB_WS_ENV, BULLETIN_WS_ENV, ChainEndpoint, Network, NetworkConfig, PEOPLE_WS_ENV,
	StackArg, local_network,
};
use crate::{chain::ChainBackend, stack_descriptor::Roles};

const ENV_NAMES: Roles<&str> =
	Roles { people: PEOPLE_WS_ENV, asset_hub: ASSET_HUB_WS_ENV, bulletin: BULLETIN_WS_ENV };

/// Chain URL overrides; `None` leaves the network's URL in place.
#[derive(Debug)]
pub struct ChainOverrides {
	urls: Roles<Option<String>>,
}

impl ChainOverrides {
	pub fn from_env() -> Result<Self> {
		let Roles { people, asset_hub, bulletin } = ENV_NAMES.map(|name| std::env::var(name).ok());
		Self::parse([people, asset_hub, bulletin])
	}

	/// Blank values are unset. Anything else must be a `ws` or `wss` URL.
	pub fn parse([people, asset_hub, bulletin]: [Option<String>; 3]) -> Result<Self> {
		let urls = ENV_NAMES.zip(Roles { people, asset_hub, bulletin }).try_map(
			|(name, value)| -> Result<Option<String>> {
				let Some(value) =
					value.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
				else {
					return Ok(None);
				};
				let url = reqwest::Url::parse(&value)
					.map_err(|err| anyhow!("{name} is not a URL ({err}): {value}"))?;
				ensure!(
					matches!(url.scheme(), "ws" | "wss"),
					"{name} must be a ws:// or wss:// URL: {value}"
				);
				Ok(Some(value))
			},
		)?;
		Ok(Self { urls })
	}

	/// The env vars behind the overrides that are set.
	pub fn env_names_in_use(&self) -> Vec<&'static str> {
		ENV_NAMES
			.zip(self.urls.as_ref())
			.iter()
			.filter_map(|(name, url)| url.is_some().then_some(*name))
			.collect()
	}
}

/// Fails when two routing sources cannot be combined: `--network` with a
/// stack, a stack or a `HOST_CLI_*_WS` override with the local-network file,
/// and the light client with a stack, an override or the local-network file.
/// Each message prints both sources.
pub fn ensure_sources_compatible(
	stack: Option<&StackArg>,
	network: Option<Network>,
	override_names: &[&str],
	local_network: bool,
	chain_backend: ChainBackend,
) -> Result<()> {
	let local_env = local_network::CONFIG_ENV;
	let light_client = chain_backend == ChainBackend::LightClient;
	if let Some(stack) = stack {
		let source = stack.source;
		ensure!(network.is_none(), "--network cannot be used with {source}; unset one of them");
		ensure!(
			!local_network,
			"{local_env} and {source} cannot be used together: both route every chain; unset one \
			 of them"
		);
		let bundled = truapi_provider::known_networks().collect::<Vec<_>>().join(", ");
		ensure!(
			!light_client,
			"--chain-backend light-client cannot run {source} {}: the light client syncs only the \
			 networks truapi-provider bundles ({bundled}); use --chain-backend rpc",
			stack.value
		);
	}
	// The chain URL overrides point a network at a fork with the same genesis;
	// the local-network file replaces the genesis too. Applying one over the
	// other would route some roles to one network and some to another.
	ensure!(
		override_names.is_empty() || !local_network,
		"{local_env} and {} cannot be used together: the file routes every chain itself; unset \
		 one of them",
		override_names.join(", ")
	);
	ensure!(
		override_names.is_empty() || !light_client,
		"{} and --chain-backend light-client cannot be used together: the light client syncs the \
		 live chains by genesis hash; unset one of them",
		override_names.join(", ")
	);
	ensure!(
		!local_network || !light_client,
		"{local_env} and --chain-backend light-client cannot be used together: the light client \
		 resolves chains by genesis hash from its bundled catalog, which has no local network; \
		 unset one of them"
	);
	Ok(())
}

/// Replaces the identity backend base with `base` when it carries a non-empty
/// URL. Trailing slashes are stripped so path joins stay clean.
pub fn apply_backend_override(mut config: NetworkConfig, base: Option<String>) -> NetworkConfig {
	if let Some(base) = base {
		let trimmed = base.trim().trim_end_matches('/');
		if !trimmed.is_empty() {
			config.identity_backend_base = Some(trimmed.to_owned());
		}
	}
	config
}

/// Points each overridden role at its new URL.
pub fn apply_chain_overrides(config: NetworkConfig, overrides: ChainOverrides) -> NetworkConfig {
	let chains = config
		.chains
		.zip(overrides.urls)
		.map(|(chain, url)| ChainEndpoint { ws: url.unwrap_or(chain.ws), ..chain });
	NetworkConfig { chains, ..config }
}

#[cfg(test)]
mod tests;
