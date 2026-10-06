//! Opt-in routing for a disposable local copy of a supported test network.
//! Keeps the preset's product namespace while replacing every host chain route.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::network::{ChainEndpoint, NetworkConfig};

pub const CONFIG_ENV: &str = "HOST_CLI_LOCAL_NETWORK_CONFIG";

/// The embedded light client's switch (`TRUAPI_LIGHT_CLIENT=1`, #1052). It finds
/// each chain in the provider's bundled catalog by genesis hash, where a local
/// network's hashes never appear, so the two are refused together.
pub const LIGHT_CLIENT_ENV: &str = "TRUAPI_LIGHT_CLIENT";

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
pub fn resolve(base: NetworkConfig) -> Result<NetworkConfig> {
    resolve_from(
        base,
        std::env::var_os(CONFIG_ENV),
        std::env::var(LIGHT_CLIENT_ENV).ok(),
    )
}

fn resolve_from(
    base: NetworkConfig,
    path: Option<std::ffi::OsString>,
    light_client: Option<String>,
) -> Result<NetworkConfig> {
    let Some(path) = path else {
        return Ok(base);
    };
    ensure!(
        light_client.as_deref() != Some("1"),
        "{CONFIG_ENV} and {LIGHT_CLIENT_ENV}=1 cannot be used together: the light client \
         resolves chains by genesis hash from its bundled catalog, which has no local \
         network; unset one of them"
    );
    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {CONFIG_ENV} file {path:?}"))?;
    parse(base, &json)
}

fn local_url(value: &str, schemes: &[&str]) -> Result<()> {
    let url = reqwest::Url::parse(value).context("invalid local endpoint URL")?;
    ensure!(
        schemes.contains(&url.scheme()),
        "wrong local endpoint scheme"
    );
    let host = url.host_str().unwrap_or_default();
    let loopback = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    ensure!(
        loopback,
        "local-network endpoints must use loopback addresses"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "endpoint credentials are unsupported"
    );
    Ok(())
}

fn genesis(value: &str) -> Result<[u8; 32]> {
    let hex = value
        .strip_prefix("0x")
        .context("genesis must start with 0x")?;
    let bytes = hex::decode(hex).context("genesis must contain hexadecimal bytes")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("genesis must be exactly 32 bytes"))
}

fn parse(mut base: NetworkConfig, json: &str) -> Result<NetworkConfig> {
    let local: LocalNetwork = serde_json::from_str(json).context("invalid local-network config")?;
    ensure!(
        local.network == base.id,
        "local-network config must match --network {}",
        base.id
    );
    local_url(&local.identity_backend_base, &["http", "https"])?;
    let chains = [&local.people, &local.asset_hub, &local.bulletin];
    let mut hashes = Vec::with_capacity(chains.len());
    for chain in chains {
        local_url(&chain.ws, &["ws", "wss"])?;
        let hash = genesis(&chain.genesis)?;
        ensure!(
            !hashes.contains(&hash),
            "each chain must have a distinct genesis hash"
        );
        hashes.push(hash);
    }

    // NetworkConfig is Copy with process-lifetime references. Match the existing
    // backend override's ownership model; validate everything before retaining it.
    base.identity_backend_base = Box::leak(
        local
            .identity_backend_base
            .trim_end_matches('/')
            .to_owned()
            .into_boxed_str(),
    );
    base.people_ws = Box::leak(local.people.ws.into_boxed_str());
    base.asset_hub_ws = Box::leak(local.asset_hub.ws.into_boxed_str());
    base.bulletin_ws = Box::leak(local.bulletin.ws.into_boxed_str());
    base.people_genesis = hashes[0];
    base.asset_hub_genesis = hashes[1];
    base.bulletin_genesis = hashes[2];
    base.live_chain_endpoints = Box::leak(
        vec![
            ChainEndpoint {
                genesis: hashes[0],
                ws: base.people_ws,
                required_for_host: true,
            },
            ChainEndpoint {
                genesis: hashes[1],
                ws: base.asset_hub_ws,
                required_for_host: true,
            },
            ChainEndpoint {
                genesis: hashes[2],
                ws: base.bulletin_ws,
                required_for_host: true,
            },
        ]
        .into_boxed_slice(),
    );
    Ok(base)
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
    fn local_routes_cover_every_advertised_chain_for_both_presets() {
        for network in [Network::Previewnet, Network::PaseoNextV2] {
            let base = network.config();
            let resolved = parse(base, &config(base.id).to_string()).unwrap();
            assert_eq!(resolved.id, base.id);
            assert_eq!(resolved.network_suffix, base.network_suffix);
            assert_eq!(
                resolved.identity_backend_base,
                "http://127.0.0.1:8092/api/v1"
            );
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
    fn cannot_mix_namespaces_or_leave_a_chain_on_the_live_preset() {
        let base = Network::PaseoNextV2.config();
        assert!(parse(base, &config("previewnet").to_string()).is_err());
        let mut incomplete = config(base.id);
        incomplete.as_object_mut().unwrap().remove("bulletin");
        assert!(parse(base, &incomplete.to_string()).is_err());
    }

    #[test]
    fn malformed_and_duplicate_genesis_hashes_are_rejected() {
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
    fn endpoints_must_be_local_and_use_the_right_protocol() {
        assert!(local_url("ws://[::1]:10010", &["ws", "wss"]).is_ok());
        assert!(local_url("ws://localhost:10010", &["ws", "wss"]).is_ok());
        for bad in [
            "wss://previewnet.substrate.dev/people",
            "http://localhost:10010",
            "ws://alice:secret@localhost:10010",
        ] {
            assert!(local_url(bad, &["ws", "wss"]).is_err());
        }
        let mut document = config("previewnet");
        document["identity_backend_base"] =
            "https://identity-previewnet.dotspark.app/api/v1".into();
        assert!(parse(Network::Previewnet.config(), &document.to_string()).is_err());
    }

    #[test]
    fn the_light_client_is_refused_with_a_local_network() {
        let base = Network::PaseoNextV2.config();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local-network.json");
        std::fs::write(&path, config(base.id).to_string()).unwrap();
        let resolve = |light_client: Option<&str>| {
            resolve_from(
                base,
                Some(path.clone().into_os_string()),
                light_client.map(str::to_owned),
            )
        };

        let error = resolve(Some("1")).expect_err("the light client cannot route a local network");
        let message = error.to_string();
        assert!(
            message.contains(CONFIG_ENV) && message.contains(LIGHT_CLIENT_ENV),
            "{message}"
        );
        // Only `1` turns the light client on, so any other value routes locally.
        for light_client in [None, Some("0"), Some("")] {
            let resolved = resolve(light_client).unwrap();
            assert!(resolved.people_ws.starts_with("ws://127.0.0.1:"));
        }
        // Without the local config the light client's own preset is untouched.
        let live = resolve_from(base, None, Some("1".to_owned())).unwrap();
        assert_eq!(live.people_ws, base.people_ws);
    }
}
