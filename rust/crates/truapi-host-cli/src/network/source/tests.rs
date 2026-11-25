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

/// The error for a `--stack` value that cannot be used, from `--stack` itself.
async fn stack_error(value: &str) -> String {
	let stack = StackArg { value: value.to_string(), source: "--stack" };
	let source = NetworkSource::new(ChainAccess::Rpc, None, Some(stack)).unwrap();
	let Err(error) = source.resolve().await else {
		panic!("{value} resolved");
	};
	error.to_string()
}

/// A local stack file that cannot be read says so, with the system's reason.
#[tokio::test]
async fn a_missing_local_stack_says_why() {
	let directory = tempfile::tempdir().expect("temporary directory");
	let path = directory.path().join("missing.json").display().to_string();

	assert_eq!(
		stack_error(&path).await,
		format!("--stack: cannot read {path}: No such file or directory (os error 2)")
	);
}

/// `<(curl …)` gives a pipe at `/dev/fd/<n>`, a path every such stack would
/// share. The error says the pipe cannot name a stack and what to do instead.
/// The writer is closed first, so nothing waits on the pipe whatever happens.
#[cfg(unix)]
#[tokio::test]
async fn a_piped_local_stack_says_why() {
	use std::os::fd::AsRawFd as _;

	let (reader, writer) = std::io::pipe().expect("a pipe");
	drop(writer);
	let path = format!("/dev/fd/{}", reader.as_raw_fd());

	assert_eq!(
		stack_error(&path).await,
		format!(
			"--stack: {path} is not a regular file, so it cannot name a stack; save the \
			 descriptor to a file and give its path"
		)
	);
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

/// `--stack` with a built-in network's link, its bare URL or its link without
/// the key is that network, so it keeps its state in the directory
/// `--network` uses rather than starting empty in one of its own.
#[test]
fn a_built_in_stack_given_by_link_or_url_keeps_its_state_directory() {
	use clap::ValueEnum as _;
	use trinity_stack::{DEFAULT_LINK_BASE, Pointer};

	for network in Network::value_variants() {
		let name = network.to_possible_value().expect("every network has a value");
		let pointer = trinity_stack::known_stack(name.get_name()).expect("a known stack");
		let url = pointer.url().as_str().to_owned();
		let keyless_link = Pointer::new(&url, None)
			.expect("the URL is a pointer")
			.to_link(DEFAULT_LINK_BASE);
		let given = [pointer.to_link(DEFAULT_LINK_BASE), url, keyless_link];
		let bundled = network.bundled_stack(ChainBackend::Rpc);

		let state_directories = given.map(|value| {
			let (_, id) = stack_key_and_id(&value).expect("the value names a stack");
			Stack::new(id, bundled.descriptor().clone(), &ChainAccess::Rpc)
				.expect("the bundled copy runs")
				.state_directory()
				.to_owned()
		});
		assert_eq!(state_directories, [name.get_name(); 3], "{network:?}");
	}
}
