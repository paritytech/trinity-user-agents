use std::sync::atomic::Ordering;

use clap::ValueEnum as _;
use trinity_stack::{DEFAULT_LINK_BASE, Pointer};

use super::{
	super::{
		Network, STACK_ENV, StackArg,
		testing::{edit_descriptor, serve_light_specs, serve_stack_and_light_specs, test_access},
	},
	*,
};
use crate::chain::ChainBackend;

/// A stack from `source`.
fn stack_arg(source: &'static str) -> Option<StackArg> {
	Some(StackArg { value: "https://stack.example/s.json".to_string(), source })
}

/// The source of `--stack value` over `chain_backend`, keeping its state in
/// `state`.
fn stack_source(
	value: &str,
	chain_backend: ChainBackend,
	state: &std::path::Path,
) -> NetworkSource {
	let stack = StackArg { value: value.to_string(), source: "--stack" };
	NetworkSource::new(test_access(chain_backend), None, Some(stack), state.to_path_buf())
		.expect("a stack alone is a source")
}

/// `--network` picks a built-in network and a stack any other one, and the
/// error prints where the stack came from.
#[test]
fn test_network_with_stack_error() {
	let state = tempfile::tempdir().expect("temporary directory");
	for source in ["--stack", STACK_ENV] {
		let network = Some(Network::Previewnet);
		let given =
			NetworkSource::new(ChainAccess::Rpc, network, stack_arg(source), state.path().into());
		let Err(error) = given else {
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
	let state = tempfile::tempdir().expect("temporary directory");
	let source =
		stack_source(&file.path().display().to_string(), ChainBackend::LightClient, state.path());

	let stack = source.resolve().await.expect("the descriptor names its light specs");

	assert_eq!(stack.chain_backend(), ChainBackend::LightClient);
}

/// The error for a `--stack` value that cannot be used, from `--stack` itself.
async fn stack_error(value: &str) -> String {
	let state = tempfile::tempdir().expect("temporary directory");
	let Err(error) = stack_source(value, ChainBackend::Rpc, state.path()).resolve().await else {
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

/// Keeps a record of `descriptor`, fetched `age_secs` ago from `pointer`, in
/// `state`, as an earlier command would have.
fn keep_copy(state: &std::path::Path, pointer: &Pointer, descriptor: &[u8], age_secs: u64) {
	let fetched_at = crate::stack_descriptor::unix_now() - age_secs;
	let served = trinity_stack::Served::Document { bytes: descriptor, etag: None };
	let record = trinity_stack::StackReader::new(trinity_stack::Refresh::Never)
		.fetched(pointer, served, fetched_at as i64)
		.expect("the copy passes the descriptor rules")
		.stored
		.expect("a first copy is stored");
	let directory = state.join(super::super::stack::state_directory_of(&pointer.stack_id()));
	std::fs::create_dir_all(&directory).expect("a state directory");
	std::fs::write(directory.join(super::super::records::RECORD_FILE), record).expect("a record");
}

/// Keeps `network`'s shipped copy in `state` as fetched just now, so
/// resolving it fetches nothing from the real network.
fn keep_shipped_copy(state: &std::path::Path, network: Network) {
	let (link, bundled) = network.link_and_bundle();
	keep_copy(state, &Pointer::from_link(&link).expect("a built-in link"), bundled, 0);
}

/// A built-in network starts from the copy this build ships, without
/// waiting on its descriptor's server while that copy is fresh, and a product sees the network,
/// TLD, chains and contracts an app bundling the same copy shows, without the
/// relay, which the CLI does not serve.
#[tokio::test]
async fn a_built_in_network_runs_the_stack_an_app_bundling_it_runs() {
	for network in Network::value_variants() {
		let state = tempfile::tempdir().expect("temporary directory");
		keep_shipped_copy(state.path(), *network);
		let source =
			NetworkSource::new(ChainAccess::Rpc, Some(*network), None, state.path().into())
				.expect("a built-in network is a source");
		let (link, bundled) = network.link_and_bundle();
		let allowed = truapi::platform::ReservedValuesPolicy::ALLOWED;
		let app = truapi::platform::Stack::bundled(&link, bundled, &allowed)
			.expect("the bundled copy starts an app");

		let stack = source.resolve().await.expect("the bundled copy runs");

		assert_eq!(stack.core_stack(), app.without_relay(), "{network:?}");
	}
}

/// `--stack` with a built-in network's link, its bare URL or its link without
/// the key is that network, so it keeps its state in the directory
/// `--network` uses rather than starting empty in one of its own.
#[tokio::test]
async fn a_built_in_stack_given_by_link_or_url_keeps_its_state_directory() {
	for network in Network::value_variants() {
		let name = network.to_possible_value().expect("every network has a value");
		let pointer = trinity_stack::known_stack(name.get_name()).expect("a known stack");
		let url = pointer.url().as_str().to_owned();
		let keyless_link = Pointer::new(&url, None)
			.expect("the URL is a pointer")
			.to_link(DEFAULT_LINK_BASE);
		let state = tempfile::tempdir().expect("temporary directory");
		keep_shipped_copy(state.path(), *network);

		let mut state_directories = Vec::new();
		for value in [pointer.to_link(DEFAULT_LINK_BASE), url, keyless_link] {
			let stack = stack_source(&value, ChainBackend::Rpc, state.path()).resolve().await;
			state_directories
				.push(stack.expect("the bundled copy runs").state_directory().to_owned());
		}
		assert_eq!(state_directories, [name.get_name(); 3], "{network:?}");
	}
}

/// The descriptor of a stack this build ships no copy of, unsigned and served
/// from `server`, at `sequence`, with its relay and People light specs named
/// at `relay` and `people` of that server.
fn served_copy(server: &str, sequence: u32, relay: &str, people: &str) -> Vec<u8> {
	let spec_directory = tempfile::tempdir().expect("temporary directory");
	let stack = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).with_light_specs_at(
		&format!("{server}/{relay}"),
		&format!("{server}/{people}"),
		spec_directory.path(),
	);
	let mut descriptor = serde_json::to_value(stack.descriptor()).expect("a descriptor is JSON");
	descriptor["id"] = "imported".into();
	descriptor["sequence"] = sequence.into();
	serde_json::to_vec(&descriptor).expect("JSON serialises")
}

/// A stack this build ships no copy of is fetched once and kept in its state
/// directory, so the next command starts from that copy instead of waiting
/// on the descriptor's server.
#[tokio::test]
async fn an_imported_stack_is_fetched_once_and_kept() {
	let (server, _) = serve_light_specs().await;
	let copy = served_copy(&server, 1, "relay.json", "people.json");
	let (server, requests) = serve_stack_and_light_specs(vec![copy]).await;
	let state = tempfile::tempdir().expect("temporary directory");
	let source = stack_source(&format!("{server}/stack.json"), ChainBackend::Rpc, state.path());

	let first = source.resolve().await.expect("the served copy runs");
	let second = source.resolve().await.expect("the kept copy runs");

	assert_eq!((first.core_stack(), requests.load(Ordering::Relaxed)), (second.core_stack(), 1));
}

/// A light spec its server no longer serves is what the operator's
/// republish fixes, so the descriptor is fetched again and the command runs
/// on the newer copy.
#[tokio::test]
async fn a_light_spec_that_no_longer_matches_is_fixed_by_a_newer_copy() {
	let (server, _) = serve_light_specs().await;
	let swapped = served_copy(&server, 1, "people.json", "relay.json");
	let fixed = served_copy(&server, 2, "relay.json", "people.json");
	let (server, _) = serve_stack_and_light_specs(vec![swapped, fixed]).await;
	let (state, specs) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
	// Specs of its own: a spec another test kept would be found by its hash,
	// and the one the wrong URL serves never fetched.
	let light_client = ChainAccess::LightClient { spec_directory: specs.path().to_path_buf() };
	let stack = StackArg { value: format!("{server}/stack.json"), source: "--stack" };
	let source = NetworkSource::new(light_client, None, Some(stack), state.path().into()).unwrap();

	let stack = source.resolve_checked().await.expect("the newer copy names the right specs");

	let light = stack.light_client().expect("the stack runs on the light client");
	assert_eq!(
		(stack.descriptor().sequence, light.provider().has_light_spec(stack.genesis().people)),
		(2, true)
	);
}

/// A command may exit before a background fetch is stored, so a copy over an
/// hour old is fetched before the command runs, and a newer one is run
/// at once: an operator who moved a service is followed by the next command.
#[tokio::test]
async fn a_copy_over_an_hour_old_is_fetched_before_the_command_runs() {
	let (server, _) = serve_light_specs().await;
	let (older, newer) = (
		served_copy(&server, 1, "relay.json", "people.json"),
		served_copy(&server, 2, "relay.json", "people.json"),
	);
	let (server, requests) = serve_stack_and_light_specs(vec![newer]).await;
	let url = format!("{server}/stack.json");
	let state = tempfile::tempdir().expect("temporary directory");
	keep_copy(state.path(), &Pointer::new(&url, None).unwrap(), &older, 2 * 60 * 60);

	let stack = stack_source(&url, ChainBackend::Rpc, state.path()).resolve().await.unwrap();

	assert_eq!((stack.descriptor().sequence, requests.load(Ordering::Relaxed)), (2, 1));
}

/// A server that does not answer stops nothing: the command runs on the copy
/// it holds.
#[tokio::test]
async fn a_copy_over_an_hour_old_runs_when_its_server_gives_none() {
	let (server, _) = serve_light_specs().await;
	let older = served_copy(&server, 1, "relay.json", "people.json");
	let url = format!("{server}/missing.json");
	let state = tempfile::tempdir().expect("temporary directory");
	keep_copy(state.path(), &Pointer::new(&url, None).unwrap(), &older, 2 * 60 * 60);

	let stack = stack_source(&url, ChainBackend::Rpc, state.path()).resolve().await.unwrap();

	assert_eq!(stack.descriptor().sequence, 1);
}

/// A kept copy the CLI cannot run, such as one past `expiresAt`, is fetched
/// again before the command fails, since its operator may have published a
/// fix after the last fetch.
#[tokio::test]
async fn a_kept_copy_that_cannot_run_is_fetched_again() {
	let (server, _) = serve_light_specs().await;
	let mut expired: serde_json::Value =
		serde_json::from_slice(&served_copy(&server, 1, "relay.json", "people.json")).unwrap();
	expired["expiresAt"] = 1.into();
	let fixed = served_copy(&server, 2, "relay.json", "people.json");
	let (server, requests) = serve_stack_and_light_specs(vec![fixed]).await;
	let url = format!("{server}/stack.json");
	let state = tempfile::tempdir().expect("temporary directory");
	let expired = serde_json::to_vec(&expired).expect("JSON serialises");
	keep_copy(state.path(), &Pointer::new(&url, None).unwrap(), &expired, 0);

	let stack = stack_source(&url, ChainBackend::Rpc, state.path()).resolve().await.unwrap();

	assert_eq!((stack.descriptor().sequence, requests.load(Ordering::Relaxed)), (2, 1));
}

/// A server that accepts the connection and never answers delays a command
/// by the pre-start limit at most, and the command runs on the copy it holds.
#[tokio::test]
async fn a_silent_server_delays_a_command_by_the_pre_start_limit_at_most() {
	let (server, _) = serve_light_specs().await;
	let older = served_copy(&server, 1, "relay.json", "people.json");
	let url = format!(
		"{}/stack.json",
		super::super::testing::serve_silent().await.replacen("ws", "http", 1)
	);
	let state = tempfile::tempdir().expect("temporary directory");
	keep_copy(state.path(), &Pointer::new(&url, None).unwrap(), &older, 2 * 60 * 60);
	let started = std::time::Instant::now();

	let stack = stack_source(&url, ChainBackend::Rpc, state.path()).resolve().await.unwrap();

	assert_eq!(stack.descriptor().sequence, 1);
	assert!(
		started.elapsed() <
			super::super::PRE_START_FETCH_TIMEOUT + std::time::Duration::from_secs(2)
	);
}

/// A built-in network's key with another URL, such as a mirror, is that
/// network, state included, and its descriptor is fetched from the URL
/// given.
#[tokio::test]
async fn a_built_in_key_with_another_url_is_fetched_from_that_url() {
	let (link, shipped) = Network::Previewnet.link_and_bundle();
	let key = Pointer::from_link(&link)
		.unwrap()
		.key()
		.map(hex::encode)
		.expect("a built-in is signed");
	let (mirror, requests) = serve_stack_and_light_specs(Vec::new()).await;
	let mirror = Pointer::new(&format!("{mirror}/stack.json"), Some(&format!("0x{key}"))).unwrap();
	let state = tempfile::tempdir().expect("temporary directory");
	keep_copy(state.path(), &mirror, shipped, 2 * 60 * 60);
	let given = mirror.to_link(DEFAULT_LINK_BASE);

	let stack = stack_source(&given, ChainBackend::Rpc, state.path()).resolve().await.unwrap();

	assert_eq!((stack.state_directory(), requests.load(Ordering::Relaxed)), ("previewnet", 1));
}
