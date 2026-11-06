use super::{
	super::{Network, STACK_ENV, StackArg},
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
			.ensure_compatible()
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
		.ensure_compatible()
		.expect_err("light client accepted with a stack");
	assert!(error.to_string().contains(STACK_ENV));
	assert!(
		build_source(Some(STACK_ENV), None, ChainBackend::Rpc)
			.ensure_compatible()
			.is_ok()
	);
	assert!(build_source(None, None, ChainBackend::LightClient).ensure_compatible().is_ok());
}
