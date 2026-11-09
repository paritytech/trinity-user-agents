use std::sync::atomic::Ordering;

use clap::ValueEnum as _;

use super::*;
use crate::{
	chain::ChainBackend,
	network::{Network, PEOPLE_LIGHT_SPEC, RELAY_LIGHT_SPEC, serve_light_specs},
};

fn genesis(spec: &str) -> [u8; 32] {
	trinity_stack::light_spec_genesis(spec.as_bytes()).expect("the fixture is a light spec")
}

/// The bundled paseo-next-v2 stack with its relay and People served by
/// `server`.
fn stack_served_by(server: &str) -> Stack {
	Network::PaseoNextV2
		.bundled_stack(ChainBackend::Rpc)
		.with_light_specs_at(&format!("{server}/relay.json"), &format!("{server}/people.json"))
}

fn light_client(stack: &Stack, directory: &Path) -> StackLightClient {
	StackLightClient::new(&stack.stack_id(), stack.descriptor(), directory.into())
		.expect("distinct genesis hashes")
}

/// A parachain needs its relay's spec too, and a chain dropped between uses
/// would sync again from its checkpoint and start with no peers.
#[tokio::test]
async fn connecting_fetches_the_chain_and_its_relay_and_keeps_them_running() {
	let (server, requests) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let light = light_client(&stack_served_by(&server), directory.path());
	let people = genesis(PEOPLE_LIGHT_SPEC);

	drop(light.connect(people).await.expect("People syncs through the stack's relay"));

	assert_eq!(requests.load(Ordering::Relaxed), 2, "the relay's spec and People's");
	assert_eq!(light.provider().relay_of(people), Some(genesis(RELAY_LIGHT_SPEC)));
	assert!(light.provider().is_connected(people));
}

/// A run must not depend on the server a spec came from: the stack starts from
/// what the CLI last checked.
#[tokio::test]
async fn a_checked_spec_is_kept_for_the_next_run() {
	let (server, requests) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let stack = stack_served_by(&server);
	let relay = genesis(RELAY_LIGHT_SPEC);

	for _ in 0..2 {
		light_client(&stack, directory.path())
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
	let stack = stack_served_by(&server);
	let relay = genesis(RELAY_LIGHT_SPEC);
	light_client(&stack, directory.path())
		.load_spec(relay)
		.await
		.expect("spec matches");
	let kept = std::fs::read_dir(directory.path())
		.expect("the spec directory exists")
		.next()
		.expect("one spec is kept")
		.expect("readable entry")
		.path();
	std::fs::write(&kept, PEOPLE_LIGHT_SPEC).expect("overwrite the kept spec");

	light_client(&stack, directory.path())
		.load_spec(relay)
		.await
		.expect("spec matches");

	assert_eq!(requests.load(Ordering::Relaxed), 2);
	assert_eq!(std::fs::read_to_string(&kept).expect("kept spec"), RELAY_LIGHT_SPEC);
}

/// A wrong spec names its chain and is never kept, so it cannot seed a later
/// run. An unreachable one fails the connect without calling the stack wrong.
#[tokio::test]
async fn a_wrong_spec_is_refused_and_an_unreachable_one_is_not_wrong() {
	let (server, _) = serve_light_specs().await;
	let directory = tempfile::tempdir().expect("temporary directory");
	let bundled = Network::PaseoNextV2.bundled_stack(ChainBackend::Rpc);
	let relay = genesis(RELAY_LIGHT_SPEC);

	let swapped = bundled.with_light_specs_at(&format!("{server}/people.json"), &server);
	let wrong = light_client(&swapped, directory.path()).load_spec(relay).await;
	assert!(
		matches!(
			&wrong,
			Err(MissingSpec::Wrong(ProviderError::LightSpec { role, genesis, .. }))
				if role == "Relay" && *genesis == relay
		),
		"{wrong:?}"
	);
	assert!(
		std::fs::read_dir(directory.path()).map_or(true, |mut kept| kept.next().is_none()),
		"a refused spec is not kept"
	);

	let moved = bundled.with_light_specs_at(&format!("{server}/moved.json"), &server);
	let unreachable = light_client(&moved, directory.path()).connect(relay).await.err();
	assert!(
		matches!(&unreachable, Some(ProviderError::Transport { reason }) if reason.contains("Relay")),
		"{unreachable:?}"
	);
}

/// pcf-devnet and paseo-next-v2 share the Paseo relay's genesis hash and name
/// different spec files for it, so one client per stack is what keeps a stack
/// on its own relay spec.
#[test]
fn stacks_that_share_a_relay_genesis_get_their_own_light_client() {
	let relay_genesis = |network: &Network| {
		let stack = network.bundled_stack(ChainBackend::LightClient);
		stack.descriptor().chain("Relay").expect("a stack has a relay").genesis()
	};
	assert_eq!(relay_genesis(&Network::PaseoNextV2), relay_genesis(&Network::PcfDevnet));

	let clients: Vec<_> = Network::value_variants()
		.iter()
		.map(|network| {
			let stack = network.bundled_stack(ChainBackend::LightClient);
			StackLightClient::of(&stack).expect("distinct genesis hashes")
		})
		.collect();

	for (index, client) in clients.iter().enumerate() {
		for other in &clients[index + 1..] {
			assert!(!Arc::ptr_eq(client, other));
		}
	}
}
