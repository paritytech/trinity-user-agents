use super::{
	super::{Network, STACK_ENV, StackArg},
	*,
};

/// A stack from `source`.
fn stack_arg(source: &'static str) -> Option<StackArg> {
	Some(StackArg { value: "https://stack.example/s.json".to_string(), source })
}

/// `--network` picks a built-in network and a stack any other one, and the
/// error prints where the stack came from.
#[test]
fn test_network_with_stack_error() {
	for source in ["--stack", STACK_ENV] {
		let Err(error) = NetworkSource::new(
			ChainBackend::Rpc,
			Some(Network::Previewnet),
			stack_arg(source),
			None,
		) else {
			panic!("--network accepted with {source}");
		};
		assert!(error.to_string().contains(source));
	}
}

/// The light client has chain specs only for the networks truapi-provider
/// bundles, so a stack, whatever its namespace, fails before the command
/// starts rather than on every connect.
#[test]
fn test_light_client_with_stack_error() {
	let Err(error) =
		NetworkSource::new(ChainBackend::LightClient, None, stack_arg(STACK_ENV), None)
	else {
		panic!("light client accepted with a stack");
	};
	assert!(error.to_string().contains(STACK_ENV));
	assert!(NetworkSource::new(ChainBackend::Rpc, None, stack_arg(STACK_ENV), None).is_ok());
	assert!(NetworkSource::new(ChainBackend::LightClient, None, None, None).is_ok());
}
