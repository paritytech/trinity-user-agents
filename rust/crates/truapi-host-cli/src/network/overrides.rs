//! What the environment changes on a resolved network: `HOST_CLI_*_WS` and
//! the identity backend override.

use anyhow::{Result, anyhow, ensure};

use super::{ChainEndpoint, NetworkConfig};
use crate::stack_descriptor::Roles;

/// Env var overriding the People chain's WebSocket URL, for a fork of the
/// resolved network with the same genesis hash.
pub const PEOPLE_WS_ENV: &str = "HOST_CLI_PEOPLE_WS";
/// Env var overriding the Asset Hub chain's WebSocket URL, for a fork of the
/// resolved network with the same genesis hash.
pub const ASSET_HUB_WS_ENV: &str = "HOST_CLI_ASSET_HUB_WS";
/// Env var overriding the Bulletin chain's WebSocket URL, for a fork of the
/// resolved network with the same genesis hash.
pub const BULLETIN_WS_ENV: &str = "HOST_CLI_BULLETIN_WS";

const ENV_NAMES: Roles<&str> =
	Roles { people: PEOPLE_WS_ENV, asset_hub: ASSET_HUB_WS_ENV, bulletin: BULLETIN_WS_ENV };

/// Chain URL overrides; `None` leaves the network's URL in place.
#[derive(Debug)]
pub struct ChainOverrides {
	urls: Roles<Option<String>>,
}

impl ChainOverrides {
	/// The overrides `HOST_CLI_*_WS` set. Fails as [`Self::parse`] does.
	pub fn from_env() -> Result<Self> {
		let Roles { people, asset_hub, bulletin } = ENV_NAMES.map(|name| std::env::var(name).ok());
		Self::parse([people, asset_hub, bulletin])
	}

	/// The overrides for People, Asset Hub and Bulletin, in that order. Blank
	/// values are unset. Fails when any other value is not a `ws` or `wss` URL.
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

	/// `config` with each overridden role pointed at its new URL.
	pub fn apply(self, config: NetworkConfig) -> NetworkConfig {
		let chains = config
			.chains
			.zip(self.urls)
			.map(|(chain, url)| ChainEndpoint { ws: url.unwrap_or(chain.ws), ..chain });
		NetworkConfig { chains, ..config }
	}
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

#[cfg(test)]
mod tests;
