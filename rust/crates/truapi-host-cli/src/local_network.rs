//! Opt-in routing for a disposable local copy of a supported test network.
//! Keeps the preset's product namespace while replacing every host chain route.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{
	chain::ChainBackend,
	network::{ChainEndpoint, NetworkConfig},
};

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

/// An absent opt-in leaves live presets unchanged. An invalid opt-in is an error;
/// it must never silently fall back to a live chain.
pub fn resolve(base: NetworkConfig, chain_overrides: &[&str]) -> Result<NetworkConfig> {
	resolve_from(base, config_path(), chain_overrides)
}

/// The local-network file's path. A blank value is unset, like the other overrides.
pub fn config_path() -> Option<std::ffi::OsString> {
	non_blank(std::env::var_os(CONFIG_ENV))
}

fn non_blank(path: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
	path.filter(|path| !path.to_string_lossy().trim().is_empty())
}

fn resolve_from(
	base: NetworkConfig,
	path: Option<std::ffi::OsString>,
	chain_overrides: &[&str],
) -> Result<NetworkConfig> {
	let Some(path) = path else {
		return Ok(base);
	};
	ensure!(
		base.chain_backend != ChainBackend::LightClient,
		"{CONFIG_ENV} and --chain-backend light-client cannot be used together: the light \
         client resolves chains by genesis hash from its bundled catalog, which has no local \
         network; unset one of them"
	);
	// The chain URL overrides point a preset at a fork that keeps its genesis; the
	// local-network file replaces the genesis too. Applying one over the other
	// would route some roles to one network and some to another.
	ensure!(
		chain_overrides.is_empty(),
		"{CONFIG_ENV} and {} cannot be used together: the file routes every chain \
         itself; unset one of them",
		chain_overrides.join(", ")
	);
	let json = std::fs::read_to_string(&path)
		.with_context(|| format!("cannot read {CONFIG_ENV} file {path:?}"))?;
	parse(base, &json)
}

fn local_url(value: &str, schemes: &[&str]) -> Result<()> {
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

fn genesis(value: &str) -> Result<[u8; 32]> {
	let hex = value.strip_prefix("0x").context("genesis must start with 0x")?;
	let bytes = hex::decode(hex).context("genesis must contain hexadecimal bytes")?;
	bytes
		.try_into()
		.map_err(|_| anyhow::anyhow!("genesis must be exactly 32 bytes"))
}

fn parse(mut base: NetworkConfig, json: &str) -> Result<NetworkConfig> {
	let local: LocalNetwork = serde_json::from_str(json).context("invalid local-network config")?;
	ensure!(local.network == base.id, "local-network config must match --network {}", base.id);
	local_url(&local.identity_backend_base, &["http", "https"])?;
	let chains = [&local.people, &local.asset_hub, &local.bulletin];
	let mut hashes = Vec::with_capacity(chains.len());
	for chain in chains {
		local_url(&chain.ws, &["ws", "wss"])?;
		let hash = genesis(&chain.genesis)?;
		ensure!(!hashes.contains(&hash), "each chain must have a distinct genesis hash");
		hashes.push(hash);
	}

	// NetworkConfig is Copy with process-lifetime references. Match the existing
	// backend override's ownership model; validate everything before retaining it.
	base.identity_backend_base = Some(Box::leak(
		local.identity_backend_base.trim_end_matches('/').to_owned().into_boxed_str(),
	));
	base.people_ws = Box::leak(local.people.ws.into_boxed_str());
	base.asset_hub_ws = Box::leak(local.asset_hub.ws.into_boxed_str());
	base.bulletin_ws = Box::leak(local.bulletin.ws.into_boxed_str());
	base.people_genesis = hashes[0];
	base.asset_hub_genesis = hashes[1];
	base.bulletin_genesis = hashes[2];
	base.live_chain_endpoints = Box::leak(
		vec![
			ChainEndpoint { genesis: hashes[0], ws: base.people_ws, required_for_host: true },
			ChainEndpoint { genesis: hashes[1], ws: base.asset_hub_ws, required_for_host: true },
			ChainEndpoint { genesis: hashes[2], ws: base.bulletin_ws, required_for_host: true },
		]
		.into_boxed_slice(),
	);
	Ok(base)
}

#[cfg(test)]
mod tests;
