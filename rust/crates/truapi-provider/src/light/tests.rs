use core::time::Duration;
use std::sync::Arc;

use futures::{
	FutureExt,
	executor::block_on,
	future::{self, Either},
	stream::StreamExt,
};

use crate::{ChainSource, EmbeddedChainProvider, platform::ChainProvider};

/// Real relay-chain spec (checkpoint included) vendored from smoldot's
/// demo specs: `add_chain` and spec-local JSON-RPC queries succeed without
/// any network access.
const RELAY_SPEC: &str = include_str!("../../tests/fixtures/paseo.json");

/// Parachain of [`RELAY_SPEC`], used to exercise the relay-add path.
const PARACHAIN_SPEC: &str = include_str!("../../tests/fixtures/paseo_people.json");

const RELAY_GENESIS: [u8; 32] = [1; 32];
const PARACHAIN_GENESIS: [u8; 32] = [2; 32];

fn frame_id(frame: &str) -> serde_json::Value {
	let frame: serde_json::Value = serde_json::from_str(frame).expect("valid JSON");
	frame["id"].clone()
}

fn offline_provider() -> EmbeddedChainProvider {
	EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.build()
}

/// The blocker this file exists to prevent. The relay under a parachain has no
/// connection of its own, so anything counting handed-out connections
/// cannot see it, and nothing would ever store the one chain that
/// actually warp syncs.
#[test]
fn a_relay_behind_a_parachain_counts_as_added() {
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.build();

	let connection =
		block_on(provider.connect(PARACHAIN_GENESIS)).expect("offline add_chain succeeds");
	assert!(provider.is_connected(PARACHAIN_GENESIS), "the chain that was asked for is running");
	assert!(
		provider.is_connected(RELAY_GENESIS),
		"so is the relay it was brought up behind, which nothing connected to"
	);

	connection.close();
	assert!(!provider.is_connected(PARACHAIN_GENESIS));
	assert!(!provider.is_connected(RELAY_GENESIS), "the relay goes when its last parachain does");
}

/// A chain is running only while something holds it, and a second close
/// must not underflow the count.
#[test]
fn a_chain_is_added_until_its_last_connection_closes() {
	let provider = offline_provider();
	let first = block_on(provider.connect(RELAY_GENESIS)).expect("first connect");
	let second = block_on(provider.connect(RELAY_GENESIS)).expect("second connect");
	assert!(provider.is_connected(RELAY_GENESIS));

	first.close();
	assert!(provider.is_connected(RELAY_GENESIS), "one closed connection does not stop the chain");

	second.close();
	assert!(!provider.is_connected(RELAY_GENESIS));
	second.close();
	assert!(!provider.is_connected(RELAY_GENESIS), "close is idempotent");
}

/// A chain connected once per use would otherwise restart with no peers each
/// time, and its relay with it.
#[test]
fn a_kept_chain_outlives_its_connections() {
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.keep_chains_running()
		.build();

	for _ in 0..2 {
		block_on(provider.connect(PARACHAIN_GENESIS))
			.expect("parachain connects")
			.close();
	}

	for genesis in [PARACHAIN_GENESIS, RELAY_GENESIS] {
		assert!(provider.is_connected(genesis));
		let mut lifecycle = provider.lifecycle(genesis).expect("the kept chain is running");
		block_on(lifecycle.next()).expect("the kept chain reports its state");
	}
}

/// The ceiling is there for a consumer that never closes what it opens, so
/// what matters is both halves: that the connect over it is refused, and
/// that closing one hands the slot back instead of spending it for the
/// life of the process.
#[test]
fn connections_are_capped_and_closing_one_frees_a_slot() {
	let provider = offline_provider();
	let mut held = Vec::new();
	for _ in 0..super::MAX_CONNECTIONS {
		held.push(block_on(provider.connect(RELAY_GENESIS)).expect("connect under the cap"));
	}

	let error = block_on(provider.connect(RELAY_GENESIS))
		.err()
		.expect("a connect over the cap must fail");
	assert_eq!(
		error.to_string(),
		format!("the light client already holds {} connections", super::MAX_CONNECTIONS)
	);

	held.pop().expect("one to close").close();
	block_on(provider.connect(RELAY_GENESIS)).expect("the closed connection freed its slot");
}

/// Dropping a handle without closing it is exactly what the consumer the
/// ceiling exists for does, and `Drop` is the only thing that frees the
/// slot on that path. Without it the ceiling stops being a bound on live
/// connections and becomes a budget of [`MAX_CONNECTIONS`] for the life of
/// the process.
#[test]
fn dropping_a_handle_frees_its_slot() {
	let provider = offline_provider();
	let mut held = Vec::new();
	for _ in 0..super::MAX_CONNECTIONS {
		held.push(block_on(provider.connect(RELAY_GENESIS)).expect("connect under the cap"));
	}

	drop(held.pop().expect("one to drop"));
	block_on(provider.connect(RELAY_GENESIS)).expect("the dropped handle freed its slot");
}

/// Why the count is its own field rather than a sum over the chain
/// refcounts: a parachain raises two of those, its own and the relay it
/// borrows, so summing them would spend the ceiling twice as fast as
/// connections are actually handed out.
#[test]
fn a_parachain_connection_spends_one_slot() {
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.build();

	let mut held = Vec::new();
	for _ in 0..super::MAX_CONNECTIONS {
		held.push(block_on(provider.connect(PARACHAIN_GENESIS)).expect("connect under the cap"));
	}

	let error = block_on(provider.connect(PARACHAIN_GENESIS))
		.err()
		.expect("a connect over the cap must fail");
	assert_eq!(
		error.to_string(),
		format!("the light client already holds {} connections", super::MAX_CONNECTIONS)
	);
}

/// A chain that was never connected is not running, so there is nothing to
/// snapshot and a stored blob can still be consumed.
#[test]
fn an_unconnected_chain_is_not_added() {
	let provider = offline_provider();
	assert!(!provider.is_connected(RELAY_GENESIS));
}

#[test]
fn garbage_chain_spec_is_an_error() {
	let provider = EmbeddedChainProvider::builder()
		.chain([1; 32], ChainSource::light_client("not a chain spec").build())
		.build();
	let error = block_on(provider.connect([1; 32]))
		.err()
		.expect("a malformed chain spec must fail to connect");
	assert!(error.to_string().contains("failed to add a chain"));
}

#[test]
fn unknown_relay_is_an_error() {
	let provider = EmbeddedChainProvider::builder()
		.parachain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build(), [9; 32])
		.build();
	let error = block_on(provider.connect(RELAY_GENESIS))
		.err()
		.expect("an unregistered relay must fail to connect");
	assert!(error.to_string().contains("not a registered light-client chain"));
}

#[test]
fn chain_name_round_trips_without_a_network() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut responses = connection.responses();
	connection.send(
		r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
	);
	let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
	assert!(response.contains("Paseo Testnet"), "unexpected response: {response}");
}

#[test]
fn parachain_reuses_its_registered_relay() {
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.build();
	// Two connects: the second must reuse the cached relay ChainId.
	for _ in 0..2 {
		let connection = block_on(provider.connect(PARACHAIN_GENESIS))
			.expect("parachain add_chain succeeds with its relay registered");
		let mut responses = connection.responses();
		connection.send(
			r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
		);
		let response = block_on(responses.next()).expect("smoldot answers spec-local queries");
		assert!(response.contains("Paseo People"), "unexpected response: {response}");
	}
}

#[test]
fn relay_is_reclaimed_when_the_last_parachain_closes() {
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.build();
	let first = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
	let second = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
	assert_eq!(provider.relay_count(), 1, "both connections share one relay");
	first.close();
	assert_eq!(provider.relay_count(), 1, "the relay stays while a parachain connection is live");
	second.close();
	assert_eq!(
		provider.relay_count(),
		0,
		"the relay is reclaimed after the last parachain connection closes"
	);
}

#[test]
fn a_failed_parachain_add_releases_its_relay() {
	// The relay entry is a chain whose spec id is not the one the parachain
	// declares, so smoldot refuses the parachain with NoRelayChainFound
	// after the relay has already been added and referenced.
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(PARACHAIN_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.build();
	block_on(provider.connect(PARACHAIN_GENESIS))
		.err()
		.expect("a parachain whose relay does not match must fail to connect");
	assert_eq!(provider.relay_count(), 0, "the relay must not leak when the parachain add fails");
}

/// A consumer that never reads must stop being accepted work.
///
/// smoldot would queue these without limit: its request channel is unbounded
/// and its service stalls once the bounded response channel backs up, so the
/// only thing standing between a runaway `send()` loop and unbounded memory
/// is the frame budget this asserts.
#[test]
fn sending_without_draining_is_refused_once_the_budget_is_spent() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	// Held but deliberately never polled, which is what makes frames pile up.
	let mut responses = connection.responses();

	let overshoot = 64;
	for id in 0..super::MAX_UNDELIVERED_FRAMES + overshoot {
		connection.send(format!(
			r#"{{"jsonrpc":"2.0","id":{id},"method":"chainSpec_v1_chainName","params":[]}}"#
		));
	}

	// Drain what is buffered and count the refusals among it.
	let mut refusals = 0;
	while let Some(Some(frame)) = block_on(async { Some(responses.next().await) }) {
		if frame.contains("response queue full") {
			refusals += 1;
		}
		if refusals >= overshoot {
			break;
		}
	}
	assert_eq!(
		refusals, overshoot,
		"every request past the budget must come back as an error frame"
	);
}

/// `rpc_methods` stands in for a request that needs the synced chain.
/// smoldot answers it at once, so only the hold explains it coming back
/// after the chain-spec query sent behind it. Offline, the test platform
/// lets the chain settle as synced within a fraction of a second, which is
/// what releases the hold.
#[test]
fn requests_wait_for_the_chain_to_sync_and_keep_their_order() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut responses = connection.responses();
	for id in 1..=2 {
		connection
			.send(format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"rpc_methods","params":[]}}"#));
	}
	connection.send(
		r#"{"jsonrpc":"2.0","id":3,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
	);

	let mut next_id = || frame_id(&block_on(responses.next()).expect("the connection stays alive"));
	assert_eq!(next_id(), 3, "the chain-spec query is not held");
	assert_eq!(
		[next_id(), next_id()],
		[1, 2],
		"held requests go out in send order once the chain syncs"
	);
}

/// Statements are gossiped with peers and never consult the chain, so a
/// statement subscription opened during a warp sync must not wait it out.
/// It is answered while the `rpc_methods` sent ahead of it is still held.
#[test]
fn statement_requests_are_not_held() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut responses = connection.responses();
	connection.send(r#"{"jsonrpc":"2.0","id":1,"method":"rpc_methods","params":[]}"#.to_owned());
	connection.send(
		r#"{"jsonrpc":"2.0","id":2,"method":"statement_subscribeStatement","params":["any"]}"#
			.to_owned(),
	);

	let frame = block_on(responses.next()).expect("the connection stays alive");
	assert_eq!(frame_id(&frame), 2, "the statement subscription is not held");
}

/// A chain that has not synced must not hold requests without limit: the
/// ones past the budget are refused, and the ones within it are answered
/// once the chain syncs.
#[test]
fn held_requests_count_against_the_budget() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut responses = connection.responses();

	let overshoot = 64;
	let sent = super::MAX_UNDELIVERED_FRAMES + overshoot;
	for id in 0..sent {
		connection
			.send(format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"rpc_methods","params":[]}}"#));
	}

	let refusals = (0..sent)
		.map(|_| block_on(responses.next()).expect("every request is answered"))
		.filter(|frame| frame.contains("response queue full"))
		.count();
	assert_eq!(refusals, overshoot);
}

/// The hold ends on the first `Ready` and on nothing before it: a chain
/// that is connecting or warp syncing still answers from a state that has
/// not caught up.
#[test]
fn the_hold_ends_on_the_first_ready_and_no_earlier() {
	use smoldot_light::lifecycle_service::{LifecycleService, Phase};

	let service = LifecycleService::new();
	let mut ready = Box::pin(super::first_ready(service.subscribe()));
	assert!(ready.as_mut().now_or_never().is_none(), "connecting holds");

	block_on(service.update(|state| state.phase = Phase::Syncing { at: 1, target: 9 }));
	assert!(ready.as_mut().now_or_never().is_none(), "syncing holds");

	block_on(service.update(|state| state.phase = Phase::Ready));
	assert!(ready.now_or_never().is_some(), "ready releases");
}

/// A host that polls a finished watch again, or twice at once, must get
/// the end again rather than a panic from polling an ended stream.
#[test]
fn a_lifecycle_keeps_reporting_its_end() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut lifecycle =
		provider.lifecycle(RELAY_GENESIS).expect("a connected chain has a lifecycle");
	connection.close();
	while block_on(lifecycle.next()).is_some() {}
	assert_eq!(block_on(lifecycle.next()), None);
}

/// A host shows sync progress through this stream, so it must start with
/// where the chain is now and end once nothing runs the chain, rather than
/// leave the host waiting on a chain that is gone.
#[test]
fn a_lifecycle_starts_with_the_current_state_and_ends_with_the_chain() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut lifecycle =
		provider.lifecycle(RELAY_GENESIS).expect("a connected chain has a lifecycle");
	block_on(lifecycle.next()).expect("the current state comes first");

	connection.close();
	let deadline = futures_timer::Delay::new(Duration::from_secs(10));
	let ended = block_on(future::select(lifecycle.count(), deadline));
	assert!(matches!(ended, Either::Left(_)), "the stream ends once the chain stops running");
}

/// The relay behind a parachain has no connection of its own, and is the
/// only chain in the pair that reports warp sync progress.
#[test]
fn the_relay_behind_a_parachain_has_a_lifecycle() {
	let provider = EmbeddedChainProvider::builder()
		.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
		.parachain(
			PARACHAIN_GENESIS,
			ChainSource::light_client(PARACHAIN_SPEC).build(),
			RELAY_GENESIS,
		)
		.build();
	let _connection = block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
	let mut lifecycle = provider.lifecycle(RELAY_GENESIS).expect("the implicit relay is running");
	block_on(lifecycle.next()).expect("the relay reports its state");
}

/// A second connection keeps the chain running after the first closes, so
/// the lifecycle must still be reachable through the one that is left.
#[test]
fn a_lifecycle_is_reachable_while_any_connection_holds_the_chain() {
	let provider = offline_provider();
	let first = block_on(provider.connect(RELAY_GENESIS)).expect("first connect");
	let _second = block_on(provider.connect(RELAY_GENESIS)).expect("second connect");
	first.close();
	let mut lifecycle = provider.lifecycle(RELAY_GENESIS).expect("the chain is still running");
	block_on(lifecycle.next()).expect("the remaining connection reports its state");
}

#[test]
fn an_unconnected_chain_has_no_lifecycle() {
	let provider = offline_provider();
	let error = provider.lifecycle(RELAY_GENESIS).err().expect("nothing runs the chain");
	assert!(error.to_string().contains("connect to it first"), "unexpected: {error}");
}

#[test]
fn close_is_idempotent_and_ends_the_stream() {
	let provider = offline_provider();
	let connection = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut responses = connection.responses();
	connection.close();
	connection.close();
	assert_eq!(block_on(responses.next()), None);
	// A late send must not panic on the removed chain.
	connection.send(
		r#"{"jsonrpc":"2.0","id":2,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
	);
}

#[test]
fn concurrent_parachain_connects_share_and_reclaim_the_relay() {
	// Many threads race the lazy client init and the shared relay refcount;
	// once every connection closes the relay must be fully reclaimed, with
	// no panic and no leak.
	let provider = Arc::new(
		EmbeddedChainProvider::builder()
			.chain(RELAY_GENESIS, ChainSource::light_client(RELAY_SPEC).build())
			.parachain(
				PARACHAIN_GENESIS,
				ChainSource::light_client(PARACHAIN_SPEC).build(),
				RELAY_GENESIS,
			)
			.build(),
	);
	let threads: Vec<_> = (0..8)
		.map(|_| {
			let provider = Arc::clone(&provider);
			std::thread::spawn(move || {
				let connection =
					block_on(provider.connect(PARACHAIN_GENESIS)).expect("parachain connects");
				connection.close();
			})
		})
		.collect();
	for thread in threads {
		thread.join().expect("connect thread does not panic");
	}
	assert_eq!(provider.relay_count(), 0, "no relay leaks under contention");
}

#[test]
fn connections_to_the_same_chain_are_isolated() {
	let provider = offline_provider();
	let first = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let second = block_on(provider.connect(RELAY_GENESIS)).expect("offline add_chain succeeds");
	let mut second_responses = second.responses();
	first.close();
	second.send(
		r#"{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_chainName","params":[]}"#.to_owned(),
	);
	let response = block_on(second_responses.next()).expect("the second connection stays alive");
	assert!(response.contains("Paseo Testnet"), "unexpected response: {response}");
}
