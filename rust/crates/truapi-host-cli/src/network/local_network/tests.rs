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
