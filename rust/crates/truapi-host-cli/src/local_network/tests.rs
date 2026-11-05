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
	document["identity_backend_base"] = "https://identity-previewnet.dotspark.app/api/v1".into();
	assert!(parse(Network::Previewnet.config(), &document.to_string()).is_err());
}

#[test]
fn the_light_client_is_refused_with_a_local_network() {
	let rpc = Network::PaseoNextV2.config();
	let light = NetworkConfig { chain_backend: ChainBackend::LightClient, ..rpc };
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("local-network.json");
	std::fs::write(&path, config(rpc.id).to_string()).unwrap();
	let resolve =
		|base: NetworkConfig| resolve_from(base, Some(path.clone().into_os_string()), &[]);

	let error = resolve(light).expect_err("the light client cannot route a local network");
	let message = error.to_string();
	assert!(
		message.contains(CONFIG_ENV) && message.contains("--chain-backend light-client"),
		"{message}"
	);
	let resolved = resolve(rpc).unwrap();
	assert!(resolved.people_ws.starts_with("ws://127.0.0.1:"));
	// Without the local config the light client's own preset is untouched.
	let live = resolve_from(light, None, &[]).unwrap();
	assert_eq!(live.people_ws, light.people_ws);
}

#[test]
fn a_blank_path_is_unset() {
	for value in ["", "  "] {
		assert!(non_blank(Some(value.into())).is_none());
	}
	assert!(non_blank(Some("/tmp/local-network.json".into())).is_some());
}

#[test]
fn chain_url_overrides_are_refused_with_a_local_network() {
	let base = Network::Previewnet.config();
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("local-network.json");
	std::fs::write(&path, config(base.id).to_string()).unwrap();

	let error = resolve_from(
		base,
		Some(path.clone().into_os_string()),
		&[crate::network::PEOPLE_WS_ENV, crate::network::BULLETIN_WS_ENV],
	)
	.expect_err("a fork override and a local network cannot both route the chains");
	let message = error.to_string();
	assert!(
		message.contains(CONFIG_ENV) &&
			message.contains(crate::network::PEOPLE_WS_ENV) &&
			message.contains(crate::network::BULLETIN_WS_ENV),
		"{message}"
	);
	// With no override set, the file routes the chains.
	let local = resolve_from(base, Some(path.into_os_string()), &[]).unwrap();
	assert_eq!(local.people_ws, "ws://127.0.0.1:10010");
}
