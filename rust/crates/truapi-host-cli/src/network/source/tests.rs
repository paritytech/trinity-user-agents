use super::{
	super::{
		Network, STACK_ENV, StackArg,
		overrides::{BULLETIN_WS_ENV, PEOPLE_WS_ENV},
	},
	*,
};

/// A source with a stack from `stack_source`, when set.
fn build_source(
	stack_source: Option<&'static str>,
	network: Option<Network>,
	chain_backend: ChainBackend,
) -> NetworkSource {
	let stack = stack_source
		.map(|source| StackArg { value: "https://stack.example/s.json".to_string(), source });
	NetworkSource { chain_backend, network, stack }
}

/// `--network` picks a built-in network and a stack any other one, and the
/// error prints where the stack came from.
#[test]
fn test_network_with_stack_error() {
	for stack_source in ["--stack", STACK_ENV] {
		let error = build_source(Some(stack_source), Some(Network::Previewnet), ChainBackend::Rpc)
			.ensure_compatible(&[], false)
			.expect_err("--network accepted with a stack");
		assert!(error.to_string().contains(stack_source));
	}
}

/// The light client has chain specs only for the networks truapi-provider
/// bundles, so a stack, whatever its namespace, fails before the command
/// starts rather than on every connect.
#[test]
fn test_light_client_with_stack_error() {
	let error = build_source(Some(STACK_ENV), None, ChainBackend::LightClient)
		.ensure_compatible(&[], false)
		.expect_err("light client accepted with a stack");
	assert!(error.to_string().contains(STACK_ENV));
	assert!(
		build_source(Some(STACK_ENV), None, ChainBackend::Rpc)
			.ensure_compatible(&[], false)
			.is_ok()
	);
}

/// The light client syncs the chains by genesis from its bundled specs, so
/// it cannot follow a chain rerouted to another URL.
#[test]
fn test_light_client_with_overrides_error() {
	let overrides = [PEOPLE_WS_ENV];
	let error = build_source(None, None, ChainBackend::LightClient)
		.ensure_compatible(&overrides, false)
		.expect_err("light client accepted with a chain URL override");
	assert!(error.to_string().contains(PEOPLE_WS_ENV));
	assert!(
		build_source(None, None, ChainBackend::Rpc)
			.ensure_compatible(&overrides, false)
			.is_ok()
	);
}

#[test]
fn test_light_client_with_local_network_error() {
	let error = build_source(None, None, ChainBackend::LightClient)
		.ensure_compatible(&[], true)
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
	let error = build_source(None, None, ChainBackend::Rpc)
		.ensure_compatible(&overrides, true)
		.expect_err("chain URL override accepted with a local network");
	let message = error.to_string();
	assert!(message.contains(local_network::CONFIG_ENV));
	assert!(message.contains(PEOPLE_WS_ENV) && message.contains(BULLETIN_WS_ENV));

	let error = build_source(Some("--stack"), None, ChainBackend::Rpc)
		.ensure_compatible(&[], true)
		.expect_err("stack accepted with a local network");
	assert!(error.to_string().contains("--stack"));
}
