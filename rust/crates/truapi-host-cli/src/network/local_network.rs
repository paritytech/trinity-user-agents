//! Opt-in routing for a disposable local copy of a built-in test network.
//! Runs under the network's product namespace while replacing every host chain
//! route.

use std::ffi::{OsStr, OsString};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::{ChainEndpoint, NetworkConfig};
use crate::stack_descriptor::Roles;

/// Env var with the path of the local-network JSON file; a blank value is unset.
pub const CONFIG_ENV: &str = "HOST_CLI_LOCAL_NETWORK_CONFIG";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalNetwork {
	network: String,
	identity_backend_base: String,
	people: LocalChain,
	asset_hub: LocalChain,
	bulletin: LocalChain,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalChain {
	ws: String,
	genesis: String,
}

/// The local-network file's path, or `None` when the variable is unset or blank.
pub fn config_path() -> Option<OsString> {
	std::env::var_os(CONFIG_ENV).filter(|path| !path.to_string_lossy().trim().is_empty())
}

/// `base` routed by the local-network file at `path`. Fails when `base` has
/// real identities, when the file cannot be read or breaks a rule; it never
/// falls back to a live chain.
pub fn resolve(base: NetworkConfig, path: &OsStr) -> Result<NetworkConfig> {
	ensure!(
		base.disposable_identities,
		"{CONFIG_ENV} routes a disposable local network, but {} has real identities; pass \
		 --network paseo-next-v2 or previewnet",
		base.id
	);
	let json = std::fs::read_to_string(path)
		.with_context(|| format!("cannot read {CONFIG_ENV} file {path:?}"))?;
	parse(base, &json)
}

fn ensure_local_url(value: &str, schemes: &[&str]) -> Result<()> {
	let url = reqwest::Url::parse(value).context("invalid local endpoint URL")?;
	ensure!(schemes.contains(&url.scheme()), "wrong local endpoint scheme");
	let host = url.host_str().unwrap_or_default();
	let loopback = host == "localhost" ||
		host.trim_matches(['[', ']'])
			.parse::<std::net::IpAddr>()
			.is_ok_and(|ip| ip.is_loopback());
	ensure!(loopback, "local-network endpoints must use loopback addresses");
	ensure!(
		url.username().is_empty() && url.password().is_none(),
		"endpoint credentials are unsupported"
	);
	Ok(())
}

fn parse_genesis(value: &str) -> Result<[u8; 32]> {
	let hex = value.strip_prefix("0x").context("genesis must start with 0x")?;
	let bytes = hex::decode(hex).context("genesis must contain hexadecimal bytes")?;
	bytes
		.try_into()
		.map_err(|_| anyhow::anyhow!("genesis must be exactly 32 bytes"))
}

fn parse(base: NetworkConfig, json: &str) -> Result<NetworkConfig> {
	let local: LocalNetwork = serde_json::from_str(json).context("invalid local-network config")?;
	ensure!(local.network == base.id, "local-network config must match --network {}", base.id);
	ensure_local_url(&local.identity_backend_base, &["http", "https"])?;
	let local_chains =
		Roles { people: local.people, asset_hub: local.asset_hub, bulletin: local.bulletin };
	let chains = local_chains.try_map(|chain| {
		ensure_local_url(&chain.ws, &["ws", "wss"])?;
		let genesis = parse_genesis(&chain.genesis)?;
		anyhow::Ok(ChainEndpoint { genesis, ws: chain.ws })
	})?;
	chains.ensure_distinct_genesis()?;
	Ok(NetworkConfig {
		identity_backend_base: Some(local.identity_backend_base.trim_end_matches('/').to_owned()),
		chains,
		..base
	})
}

#[cfg(test)]
mod tests;
