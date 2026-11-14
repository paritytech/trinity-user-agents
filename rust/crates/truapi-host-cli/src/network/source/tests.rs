use super::{
	super::{Network, STACK_ENV, StackArg},
	*,
};
use crate::chain::ChainBackend;

/// A stack from `source`.
fn stack_arg(source: &'static str) -> Option<StackArg> {
	Some(StackArg { value: "https://stack.example/s.json".to_string(), source })
}

/// `--network` picks a built-in network and a stack any other one, and the
/// error prints where the stack came from.
#[test]
fn test_network_with_stack_error() {
	for source in ["--stack", STACK_ENV] {
		let Err(error) =
			NetworkSource::new(ChainAccess::Rpc, Some(Network::Previewnet), stack_arg(source))
		else {
			panic!("--network accepted with {source}");
		};
		assert!(error.to_string().contains(source));
	}
}

/// The light client takes its chain specs from the stack's descriptor, so a
/// stack runs on either backend.
#[test]
fn test_stack_on_either_backend() {
	for chain_backend in [ChainBackend::Rpc, ChainBackend::LightClient] {
		let chain_access = super::super::testing::test_access(chain_backend);
		assert!(NetworkSource::new(chain_access, None, stack_arg(STACK_ENV)).is_ok());
	}
}
