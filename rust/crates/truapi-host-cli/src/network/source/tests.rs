use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
	super::{
		Network, STACK_ENV, StackArg,
		testing::{edit_descriptor, serve_light_specs, test_access},
	},
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
/// local descriptor file runs on it as a built-in does.
#[tokio::test]
async fn test_local_stack_on_the_light_client() {
	let file = tempfile::NamedTempFile::new().expect("temporary file");
	std::fs::write(file.path(), edit_descriptor(|_| {})).expect("write the descriptor");
	let stack = StackArg { value: file.path().display().to_string(), source: "--stack" };
	let source =
		NetworkSource::new(test_access(ChainBackend::LightClient), None, Some(stack)).unwrap();

	let stack = source.resolve().await.expect("the descriptor names its light specs");

	assert_eq!(stack.chain_backend(), ChainBackend::LightClient);
}

/// A bundled copy whose light spec no longer matches what its server returns
/// is what an operator's republish fixes, so the published descriptor is
/// fetched once more and the command runs on it.
#[tokio::test]
async fn test_wrong_bundled_light_spec_published_fix() {
	let (server, _) = serve_light_specs().await;
	let (relay, people) = (format!("{server}/relay.json"), format!("{server}/people.json"));
	let directory = tempfile::tempdir().expect("temporary directory");
	let bundled = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	let origin = |copy| BuiltInOrigin {
		id: "paseo-next-v2".to_owned(),
		url: "https://stack.example/s.json".to_owned(),
		copy,
	};
	let resolves = AtomicUsize::new(0);

	let stack = checked(|_| {
		let (stack, copy) = match resolves.fetch_add(1, Ordering::Relaxed) {
			0 => (
				bundled.with_light_specs_at(&people, &relay, directory.path()),
				BuiltInCopy::Unreachable,
			),
			_ => (
				bundled.with_light_specs_at(&relay, &people, directory.path()),
				BuiltInCopy::Published,
			),
		};
		async move { Ok((stack, Some(origin(copy)))) }
	})
	.await
	.expect("the published copy names the specs its server returns");

	assert_eq!(resolves.load(Ordering::Relaxed), 2);
	let light = stack.light_client().expect("the stack runs on the light client");
	assert!(light.provider().has_light_spec(stack.genesis().people));
}
