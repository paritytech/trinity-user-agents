use super::{
	super::{IDENTITY_BACKEND_BASE_ENV, Network, stack::stack_network_config, testing::edit_stack},
	*,
};
use crate::chain::ChainBackend;

#[test]
fn test_backend_override_hint() {
	let no_backend = NetworkConfig {
		identity_backend_base: None,
		..Network::PaseoNextV2.config(ChainBackend::Rpc)
	};
	let error = no_backend.identity_backend().expect_err("backend accepted where none is set");
	assert!(error.to_string().contains(IDENTITY_BACKEND_BASE_ENV));

	let overridden =
		apply_backend_override(no_backend, Some("http://localhost:8080/api/v1".to_string()));
	assert_eq!(overridden.identity_backend().ok(), Some("http://localhost:8080/api/v1"));
}

#[test]
fn backend_override_replaces_only_the_backend_base() {
	let paseo = Network::PaseoNextV2.config(ChainBackend::Rpc);
	let overridden =
		apply_backend_override(paseo.clone(), Some("http://localhost:8080/api/v1/".to_string()));

	assert_eq!(overridden.identity_backend_base.as_deref(), Some("http://localhost:8080/api/v1"));
	assert_eq!(overridden.chains, paseo.chains);
	assert_eq!(
		apply_backend_override(paseo.clone(), Some("  ".to_string())).identity_backend_base,
		paseo.identity_backend_base,
	);
}

#[test]
fn test_chain_overrides_blank_unset() {
	let overrides =
		ChainOverrides::parse([None, Some(String::new()), Some("  ".to_string())]).unwrap();
	assert!(overrides.env_names_in_use().is_empty());
	let paseo = Network::PaseoNextV2.config(ChainBackend::Rpc);
	let resolved = overrides.apply(paseo.clone());
	assert_eq!(resolved.chains, paseo.chains);
}

#[test]
fn test_chain_overrides_websocket_only() {
	for value in ["127.0.0.1:10010", "http://127.0.0.1:10010"] {
		let error = ChainOverrides::parse([None, None, Some(value.to_string())])
			.expect_err("non-ws override accepted");
		assert!(error.to_string().contains(BULLETIN_WS_ENV));
	}
}

/// An override reroutes one role of a built-in network or a stack, and the
/// genesis stays the network's, as a fork has the same one.
#[test]
fn test_chain_overrides_field_and_route() {
	let stack = stack_network_config(&edit_stack(|_| {}), ChainBackend::Rpc).unwrap();
	for network in [Network::Previewnet.config(ChainBackend::Rpc), stack] {
		let overrides = ChainOverrides::parse([
			Some(" ws://127.0.0.1:10010 ".to_string()),
			Some(" ".to_string()),
			None,
		])
		.unwrap();
		assert_eq!(overrides.env_names_in_use(), [PEOPLE_WS_ENV]);
		let overridden = overrides.apply(network.clone());
		let people = ChainEndpoint {
			genesis: network.chains.people.genesis,
			ws: "ws://127.0.0.1:10010".to_owned(),
		};
		assert_eq!(overridden.chains, Roles { people, ..network.chains });
	}
}
