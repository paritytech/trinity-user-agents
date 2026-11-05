use super::{
	super::{
		IDENTITY_BACKEND_BASE_ENV, STACK_ENV, fixtures::edit_stack, stack::stack_network_config,
	},
	*,
};

#[test]
fn test_backend_override_hint() {
	let polkadot = Network::Polkadot.config();
	let error = polkadot.identity_backend().expect_err("polkadot ships no backend");
	assert!(error.to_string().contains(IDENTITY_BACKEND_BASE_ENV));

	let overridden =
		apply_backend_override(polkadot, Some("http://localhost:8080/api/v1".to_string()));
	assert_eq!(overridden.identity_backend().ok(), Some("http://localhost:8080/api/v1"));
}

#[test]
fn backend_override_replaces_only_the_backend_base() {
	let paseo = Network::PaseoNextV2.config();
	let overridden =
		apply_backend_override(paseo.clone(), Some("http://localhost:8080/api/v1/".to_string()));

	assert_eq!(overridden.identity_backend_base.as_deref(), Some("http://localhost:8080/api/v1"));
	assert_eq!(overridden.asset_hub_ws, paseo.asset_hub_ws);
	assert_eq!(
		apply_backend_override(paseo.clone(), Some("  ".to_string())).identity_backend_base,
		paseo.identity_backend_base,
	);
}

fn stack_arg(source: &'static str) -> StackArg {
	StackArg { value: "https://stack.example/s.json".to_string(), source }
}

/// `--network` picks a built-in network and a stack any other one, and the
/// error prints where the stack came from.
#[test]
fn test_network_with_stack_error() {
	for source in ["--stack", STACK_ENV] {
		let stack = stack_arg(source);
		let error = ensure_sources_compatible(
			Some(&stack),
			Some(Network::Previewnet),
			&[],
			false,
			ChainBackend::Rpc,
		)
		.expect_err("--network accepted with a stack");
		assert!(error.to_string().contains(source));
	}
}

/// The light client has chain specs only for the networks truapi-provider
/// bundles, so a stack, whatever its namespace, fails before the command
/// starts rather than on every connect.
#[test]
fn test_light_client_with_stack_error() {
	let stack = stack_arg(STACK_ENV);
	let error =
		ensure_sources_compatible(Some(&stack), None, &[], false, ChainBackend::LightClient)
			.expect_err("light client accepted with a stack");
	assert!(error.to_string().contains(STACK_ENV));
	assert!(ensure_sources_compatible(Some(&stack), None, &[], false, ChainBackend::Rpc).is_ok());
}

/// The light client syncs the chains by genesis from its bundled specs, so
/// it cannot follow a chain rerouted to another URL.
#[test]
fn test_light_client_with_overrides_error() {
	let overrides = [PEOPLE_WS_ENV];
	let error = ensure_sources_compatible(None, None, &overrides, false, ChainBackend::LightClient)
		.expect_err("light client accepted with a chain URL override");
	assert!(error.to_string().contains(PEOPLE_WS_ENV));
	assert!(ensure_sources_compatible(None, None, &overrides, false, ChainBackend::Rpc).is_ok());
}

#[test]
fn test_light_client_with_local_network_error() {
	let error = ensure_sources_compatible(None, None, &[], true, ChainBackend::LightClient)
		.expect_err("light client accepted with a local network");
	let message = error.to_string();
	assert!(message.contains(local_network::CONFIG_ENV));
	assert!(message.contains("--chain-backend light-client"));
}

/// The overrides point a network at a fork with the same genesis, and the
/// local-network file replaces the genesis: mixing them would route roles
/// to two networks.
#[test]
fn test_chain_overrides_with_local_network_error() {
	let overrides = [PEOPLE_WS_ENV, BULLETIN_WS_ENV];
	let error = ensure_sources_compatible(None, None, &overrides, true, ChainBackend::Rpc)
		.expect_err("chain URL override accepted with a local network");
	let message = error.to_string();
	assert!(message.contains(local_network::CONFIG_ENV));
	assert!(message.contains(PEOPLE_WS_ENV) && message.contains(BULLETIN_WS_ENV));

	let stack = stack_arg("--stack");
	let error = ensure_sources_compatible(Some(&stack), None, &[], true, ChainBackend::Rpc)
		.expect_err("stack accepted with a local network");
	assert!(error.to_string().contains("--stack"));
}

#[test]
fn test_chain_overrides_blank_unset() {
	let overrides =
		ChainOverrides::parse([None, Some(String::new()), Some("  ".to_string())]).unwrap();
	assert!(overrides.env_names_in_use().is_empty());
	let paseo = Network::PaseoNextV2.config();
	let resolved = apply_chain_overrides(paseo.clone(), overrides);
	assert_eq!(resolved.live_chain_endpoints, paseo.live_chain_endpoints);
}

#[test]
fn test_chain_overrides_websocket_only() {
	for value in ["127.0.0.1:10010", "http://127.0.0.1:10010"] {
		let error = ChainOverrides::parse([None, None, Some(value.to_string())])
			.expect_err("non-ws override accepted");
		assert!(error.to_string().contains(BULLETIN_WS_ENV));
	}
}

/// An override reroutes one role of a built-in network or a stack, in both
/// its field and the genesis-keyed routing table, and the genesis stays the
/// network's, as a fork has the same one.
#[test]
fn test_chain_overrides_field_and_route() {
	let stack = stack_network_config(&edit_stack(|_| {})).unwrap();
	for network in [Network::Previewnet.config(), stack] {
		let overrides = ChainOverrides::parse([
			Some(" ws://127.0.0.1:10010 ".to_string()),
			Some(" ".to_string()),
			None,
		])
		.unwrap();
		assert_eq!(overrides.env_names_in_use(), [PEOPLE_WS_ENV]);
		let overridden = apply_chain_overrides(network.clone(), overrides);
		let route = |genesis: [u8; 32]| {
			overridden
				.live_chain_endpoints
				.iter()
				.find(|endpoint| endpoint.genesis == genesis)
				.map(|endpoint| endpoint.ws.as_str())
		};

		assert_eq!(overridden.people_ws, "ws://127.0.0.1:10010");
		assert_eq!(overridden.asset_hub_ws, network.asset_hub_ws);
		assert_eq!(overridden.bulletin_ws, network.bulletin_ws);
		assert_eq!(overridden.people_genesis, network.people_genesis);
		assert_eq!(route(network.people_genesis), Some("ws://127.0.0.1:10010"));
		assert_eq!(route(network.asset_hub_genesis), Some(network.asset_hub_ws.as_str()));
		assert_eq!(route(network.bulletin_genesis), Some(network.bulletin_ws.as_str()));
	}
}
