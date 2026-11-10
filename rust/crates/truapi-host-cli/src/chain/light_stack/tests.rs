use std::sync::atomic::Ordering;

use super::*;
use crate::{
	chain::ChainBackend,
	network::{Network, PEOPLE_LIGHT_SPEC, RELAY_LIGHT_SPEC, Stack, serve_light_specs},
};

fn genesis(spec: &str) -> [u8; 32] {
	trinity_stack::light_spec_genesis(spec.as_bytes()).expect("the fixture is a light spec")
}

/// The bundled paseo-next-v2 stack with its relay served at `relay_path` and
/// People at `people_path` of `server`, keeping specs in `directory`.
fn stack_served_at(server: &str, relay_path: &str, people_path: &str, directory: &Path) -> Stack {
	Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc).with_light_specs_at(
		&format!("{server}/{relay_path}"),
		&format!("{server}/{people_path}"),
		directory,
	)
}

/// The same, each chain at its own spec.
fn stack_served_by(server: &str, directory: &Path) -> Stack {
	stack_served_at(server, "relay.json", "people.json", directory)
}

fn light_client(stack: &Stack) -> &StackLightClient {
	stack.light_client().expect("the stack runs on the light client")
}

fn kept_specs(directory: &Path) -> Vec<PathBuf> {
	let Ok(entries) = std::fs::read_dir(directory) else {
		return Vec::new();
	};
	entries.map(|entry| entry.expect("readable entry").path()).collect()
}

/// A parachain needs its relay's spec too, and a chain dropped between uses
/// would sync again from its checkpoint and start with no peers.
#[tokio::test]
async fn connecting_fetches_the_chain_and_its_relay_and_keeps_them_running() {
	let (server, requests) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let stack = stack_served_by(&server, directory.path());
	let light = light_client(&stack);
	let people = genesis(PEOPLE_LIGHT_SPEC);

	drop(light.connect(people).await.expect("People syncs through the stack's relay"));

	assert_eq!(requests.load(Ordering::Relaxed), 2, "the relay's spec and People's");
	assert_eq!(light.provider().relay_of(people), Some(genesis(RELAY_LIGHT_SPEC)));
	assert!(light.provider().is_connected(people));
}

/// Connects that need the same spec at once, as three parachains need their
/// relay's, fetch it once.
#[tokio::test]
async fn concurrent_loads_fetch_a_spec_once() {
	let (server, requests) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let stack = stack_served_by(&server, directory.path());
	let light = light_client(&stack);
	let relay = genesis(RELAY_LIGHT_SPEC);

	let (first, second) = tokio::join!(light.load_spec(relay), light.load_spec(relay));

	assert!(first.is_ok() && second.is_ok());
	assert_eq!(requests.load(Ordering::Relaxed), 1);
}

/// A run must not depend on the server a spec came from: the stack starts from
/// what the CLI last checked.
#[tokio::test]
async fn a_checked_spec_is_kept_for_the_next_run() {
	let (server, requests) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let relay = genesis(RELAY_LIGHT_SPEC);

	for _ in 0..2 {
		let stack = stack_served_by(&server, directory.path());
		light_client(&stack)
			.load_spec(relay)
			.await
			.expect("the spec matches the descriptor");
	}

	assert_eq!(requests.load(Ordering::Relaxed), 1, "the second run reads the kept copy");
}

/// A kept file that no longer passes the check is not trusted over the
/// descriptor: the spec is fetched again and the file replaced.
#[tokio::test]
async fn a_kept_spec_that_fails_the_check_is_fetched_again() {
	let (server, requests) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let relay = genesis(RELAY_LIGHT_SPEC);
	let first = stack_served_by(&server, directory.path());
	light_client(&first).load_spec(relay).await.expect("spec matches");
	let [kept] = kept_specs(directory.path()).try_into().expect("one spec is kept");
	std::fs::write(&kept, PEOPLE_LIGHT_SPEC).expect("overwrite the kept spec");

	let second = stack_served_by(&server, directory.path());
	light_client(&second).load_spec(relay).await.expect("spec matches");

	assert_eq!(requests.load(Ordering::Relaxed), 2);
	assert_eq!(std::fs::read_to_string(&kept).expect("kept spec"), RELAY_LIGHT_SPEC);
}

/// A wrong spec names its chain and is never kept, so it cannot seed a later
/// run. An unreachable one fails the connect without calling the stack wrong.
#[tokio::test]
async fn a_wrong_spec_is_refused_and_an_unreachable_one_is_not_wrong() {
	let (server, _) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let relay = genesis(RELAY_LIGHT_SPEC);

	let swapped = stack_served_at(&server, "people.json", "relay.json", directory.path());
	let wrong = light_client(&swapped).load_spec(relay).await;
	assert!(
		matches!(
			&wrong,
			Err(MissingSpec::Wrong(ProviderError::LightSpec { role, genesis, error }))
				if role == "Relay" && *genesis == relay && *error == trinity_stack::Error::SpecDigest
		),
		"{wrong:?}"
	);
	assert_eq!(kept_specs(directory.path()), Vec::<PathBuf>::new(), "a refused spec is not kept");

	let moved = stack_served_at(&server, "moved.json", "people.json", directory.path());
	let unreachable = light_client(&moved).connect(relay).await.err();
	assert!(
		matches!(&unreachable, Some(ProviderError::Transport { reason }) if reason.contains("Relay")),
		"{unreachable:?}"
	);
}

/// A relay's spec is replaced with every fresher checkpoint, so the specs a
/// stack no longer names are removed when a new one is kept.
#[test]
fn keeping_a_spec_removes_the_expired_ones() {
	let directory = tempfile::tempdir().expect("temporary directory");
	let [expired, recent, new] =
		["expired.json", "recent.json", "new.json"].map(|name| directory.path().join(name));
	for path in [&expired, &recent] {
		std::fs::write(path, "{}").expect("write a kept spec");
	}
	let long_ago = SystemTime::now() - KEPT_SPEC_LIFETIME - Duration::from_secs(60);
	std::fs::File::options()
		.write(true)
		.open(&expired)
		.and_then(|file| file.set_modified(long_ago))
		.expect("age the kept spec");

	keep(&new, b"{}").expect("the directory is writable");

	let mut kept = kept_specs(directory.path());
	kept.sort();
	assert_eq!(kept, [new, recent]);
}
