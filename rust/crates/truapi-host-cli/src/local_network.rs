//! Opt-in routing for a disposable local copy of a built-in test network.
//! Runs under the network's product namespace while replacing every host chain
//! route.

use std::ffi::{OsStr, OsString};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::network::{ChainEndpoint, NetworkConfig, leak_str, route_roles};

/// Env var naming the local-network JSON file; a blank value is unset.
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

fn parse(mut base: NetworkConfig, json: &str) -> Result<NetworkConfig> {
	let local: LocalNetwork = serde_json::from_str(json).context("invalid local-network config")?;
	ensure!(local.network == base.id, "local-network config must match --network {}", base.id);
	ensure_local_url(&local.identity_backend_base, &["http", "https"])?;
	let endpoint = |chain: &LocalChain| -> Result<ChainEndpoint> {
		ensure_local_url(&chain.ws, &["ws", "wss"])?;
		let genesis = parse_genesis(&chain.genesis)?;
		Ok(ChainEndpoint { genesis, ws: leak_str(&chain.ws), required_for_host: true })
	};
	let roles = [endpoint(&local.people)?, endpoint(&local.asset_hub)?, endpoint(&local.bulletin)?];
	base.identity_backend_base = Some(leak_str(local.identity_backend_base.trim_end_matches('/')));
	route_roles(base, roles)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::network::Network;

	fn config(network: &str) -> serde_json::Value {
		serde_json::json!({
			"network": network,
			"identity_backend_base": "http://127.0.0.1:8092/api/v1/",
			"people": {"ws": "ws://127.0.0.1:10010", "genesis": format!("0x{}", "11".repeat(32))},
			"asset_hub": {"ws": "ws://127.0.0.1:10020", "genesis": format!("0x{}", "22".repeat(32))},
			"bulletin": {"ws": "ws://127.0.0.1:10030", "genesis": format!("0x{}", "33".repeat(32))}
		})
	}

	#[test]
	fn test_local_routes_presets() {
		for network in [Network::Previewnet, Network::PaseoNextV2] {
			let base = network.config();
			let resolved = parse(base, &config(base.id).to_string()).unwrap();
			assert_eq!(resolved.id, base.id);
			assert_eq!(resolved.network_suffix, base.network_suffix);
			assert_eq!(resolved.identity_backend_base, Some("http://127.0.0.1:8092/api/v1"));
			assert_eq!(resolved.live_chain_endpoints.len(), 3);
			for role in resolved.host_chain_set().chains {
				let route = resolved
					.live_chain_endpoints
					.iter()
					.find(|route| route.genesis == role.genesis_hash)
					.unwrap();
				assert_eq!(Some(route.ws), resolved.url_for_role(role.identifier));
				assert!(route.required_for_host);
				assert!(route.ws.starts_with("ws://127.0.0.1:"));
			}
		}
	}

	#[test]
	fn test_namespace_mismatch_or_missing_chain_error() {
		let base = Network::PaseoNextV2.config();
		assert!(parse(base, &config("previewnet").to_string()).is_err());
		let mut incomplete = config(base.id);
		incomplete.as_object_mut().unwrap().remove("bulletin");
		assert!(parse(base, &incomplete.to_string()).is_err());
	}

	#[test]
	fn test_genesis_malformed_or_duplicate_error() {
		let base = Network::PaseoNextV2.config();
		for value in ["0x01".to_string(), "0xzz".repeat(32), "11".repeat(32)] {
			let mut document = config(base.id);
			document["people"]["genesis"] = value.into();
			assert!(parse(base, &document.to_string()).is_err());
		}
		let mut document = config(base.id);
		document["bulletin"]["genesis"] = document["people"]["genesis"].clone();
		assert!(parse(base, &document.to_string()).is_err());
	}

	#[test]
	fn test_endpoint_loopback_scheme() {
		assert!(ensure_local_url("ws://[::1]:10010", &["ws", "wss"]).is_ok());
		assert!(ensure_local_url("ws://localhost:10010", &["ws", "wss"]).is_ok());
		for bad in [
			"wss://people.example/people",
			"http://localhost:10010",
			"ws://alice:secret@localhost:10010",
		] {
			assert!(ensure_local_url(bad, &["ws", "wss"]).is_err());
		}
		let mut document = config("previewnet");
		document["identity_backend_base"] = "https://identity.example/api/v1".into();
		assert!(parse(Network::Previewnet.config(), &document.to_string()).is_err());
	}

	/// A local copy runs on disposable identities, so a network whose
	/// identities are real cannot be routed to one.
	#[test]
	fn test_real_identity_network_error() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("local-network.json");
		std::fs::write(&path, config("polkadot").to_string()).unwrap();
		let error = resolve(Network::Polkadot.config(), path.as_os_str())
			.expect_err("local network accepted on polkadot");
		assert!(error.to_string().contains("real identities"));
		std::fs::write(&path, config("previewnet").to_string()).unwrap();
		assert!(resolve(Network::Previewnet.config(), path.as_os_str()).is_ok());
	}
}
