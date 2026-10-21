//! [`WatchedStorage`] for `ChainRuntime`, against the scripted node.

use std::sync::{
	Arc, Mutex,
	mpsc::{Receiver, channel},
};

use futures::{StreamExt, executor::block_on, stream::BoxStream};
use serde_json::json;
use sp_crypto_hashing::twox_128;

use super::{
	BEST_SUBSCRIPTION, FINALIZED_SUBSCRIPTION, GENESIS, Node, TIMEOUT, head_notification,
	method_count, serve, storage_notification, wait_for_subscriptions,
};
use crate::{
	chain::{RawStorageChanges, WatchedHead, WatchedStorage},
	chain_runtime::{ChainRuntime, RuntimeFailure},
	test_support::{ScriptedProvider, notification_sender},
};

const WATCHED: &[u8] = b"watched";
const OTHER: &[u8] = b"other";

type ReceivedRawChanges = Receiver<Result<RawStorageChanges, RuntimeFailure>>;

fn last_runtime_upgrade_key() -> Vec<u8> {
	[twox_128(b"System"), twox_128(b"LastRuntimeUpgrade")].concat()
}

/// Polls `changes` on a thread, since the node only announces heads after the
/// subscriptions are open, and hands each item over as it arrives.
fn forward_on_thread(
	mut changes: BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>,
) -> ReceivedRawChanges {
	let (sender, receiver) = channel();
	std::thread::spawn(move || {
		block_on(async move {
			while let Some(item) = changes.next().await {
				if sender.send(item).is_err() {
					return;
				}
			}
		})
	});
	receiver
}

/// Starts a watch of `keys` at `head`.
fn start_watch(chain: &ChainRuntime, head: WatchedHead, keys: &[&[u8]]) -> ReceivedRawChanges {
	let keys = keys.iter().map(|key| key.to_vec()).collect();
	let changes = block_on(chain.watch_raw_key_changes(GENESIS, head, keys)).unwrap();
	forward_on_thread(changes)
}

/// Starts a finalized watch of `keys` and waits until its head subscriptions
/// are open.
fn start_finalized_watch(
	provider: &ScriptedProvider,
	chain: &ChainRuntime,
	keys: &[&[u8]],
) -> ReceivedRawChanges {
	let received = start_watch(chain, WatchedHead::Finalized, keys);
	wait_for_subscriptions(provider);
	received
}

fn announce_head(
	node: &Arc<Mutex<Node>>,
	provider: &ScriptedProvider,
	subscription: &str,
	number: u64,
) {
	let header = node.lock().unwrap().headers[number as usize].clone();
	notification_sender(provider)
		.unbounded_send(head_notification(subscription, &header))
		.unwrap();
}

/// Pushes a `state_storage` notification at block `number` naming `changes`.
fn push_storage_notification(
	node: &Arc<Mutex<Node>>,
	provider: &ScriptedProvider,
	number: u64,
	changes: &[(&[u8], Option<&[u8]>)],
) {
	let block = node.lock().unwrap().block(number).hash;
	let changes: Vec<_> = changes
		.iter()
		.map(|(key, value)| {
			let value = value.map(|value| format!("0x{}", hex::encode(value)));
			json!([format!("0x{}", hex::encode(key)), value])
		})
		.collect();
	let notification = storage_notification(json!({"block": block, "changes": changes}));
	notification_sender(provider).unbounded_send(notification).unwrap();
}

fn store_values_at_block(node: &mut Node, number: u64, entries: &[(&[u8], &[u8])]) {
	let at = node.block(number).hash;
	let entries = entries.iter().map(|(key, value)| (key.to_vec(), value.to_vec()));
	node.storage.entry(at).or_default().extend(entries);
}

/// Blocks up to 7 with block 5 best, so a best watch's first notification
/// is at 5 and later ones can name blocks 6 and 7.
fn node_with_best_block_five() -> Node {
	let mut node = Node::new(3, 7);
	node.best = 5;
	node
}

fn next_item_within_timeout(
	received: &ReceivedRawChanges,
) -> Result<RawStorageChanges, RuntimeFailure> {
	received.recv_timeout(TIMEOUT).expect("an item before the timeout")
}

#[test]
fn each_finalized_head_reads_every_watched_key_in_one_call() {
	let mut node = Node::new(3, 5);
	store_values_at_block(&mut node, 4, &[(WATCHED, b"one")]);
	let at = node.block(4);
	let (node, provider, chain) = serve(node);
	let received = start_finalized_watch(&provider, &chain, &[WATCHED, OTHER]);

	announce_head(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	let changes = next_item_within_timeout(&received).unwrap();

	assert_eq!((changes.at, changes.value), (at, vec![(0, Some(b"one".to_vec())), (1, None)]));
	assert_eq!(method_count(&provider, "state_queryStorageAt"), 1);
}

#[test]
fn a_finalized_watch_does_not_read_at_best_heads() {
	let mut node = Node::new(3, 5);
	store_values_at_block(&mut node, 4, &[(WATCHED, b"finalized")]);
	store_values_at_block(&mut node, 5, &[(WATCHED, b"best")]);
	let at = node.block(4);
	let (node, provider, chain) = serve(node);
	let received = start_finalized_watch(&provider, &chain, &[WATCHED]);

	announce_head(&node, &provider, BEST_SUBSCRIPTION, 5);
	// Long enough for a wrongly accepted best head to be read before the
	// finalized one arrives, so the two cannot collapse into one read.
	std::thread::sleep(core::time::Duration::from_millis(200));
	announce_head(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	let changes = next_item_within_timeout(&received).unwrap();

	assert_eq!((changes.at, changes.value), (at, vec![(0, Some(b"finalized".to_vec()))]));
}

#[test]
fn a_finalized_watch_fetches_metadata_again_only_after_a_runtime_upgrade() {
	// Every metadata fetch at a block starts with a `Core_version` call, so
	// counting them counts the fetches.
	let upgrade = last_runtime_upgrade_key();
	let mut node = Node::new(3, 7);
	store_values_at_block(&mut node, 4, &[(&upgrade, b"spec 1"), (WATCHED, b"a")]);
	store_values_at_block(&mut node, 5, &[(&upgrade, b"spec 1"), (WATCHED, b"b")]);
	store_values_at_block(&mut node, 6, &[(&upgrade, b"spec 2"), (WATCHED, b"c")]);
	let (node, provider, chain) = serve(node);
	let received = start_finalized_watch(&provider, &chain, &[WATCHED]);
	let fetches_after = |number: u64| {
		announce_head(&node, &provider, FINALIZED_SUBSCRIPTION, number);
		next_item_within_timeout(&received).unwrap();
		method_count(&provider, "Core_version")
	};

	let first = fetches_after(4);
	let fetches = [fetches_after(5), fetches_after(6)];

	assert_eq!(fetches, [first, first + 1]);
}

#[test]
fn a_key_the_node_leaves_out_is_an_error_rather_than_an_absent_value() {
	let mut node = Node::new(3, 5);
	node.unanswered_keys.insert(OTHER.to_vec());
	let (node, provider, chain) = serve(node);
	let received = start_finalized_watch(&provider, &chain, &[WATCHED, OTHER]);

	announce_head(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	let error = next_item_within_timeout(&received).unwrap_err();

	let key = hex::encode(OTHER);
	assert!(error.to_string().contains(&format!("key 0x{key} at")), "{error}");
}

#[test]
fn a_best_watch_reports_the_first_notification_whole_and_later_ones_as_they_come() {
	// The node names only changed keys after the first notification, so a
	// later one is reported as is, without reading the other keys again.
	let mut node = node_with_best_block_five();
	store_values_at_block(&mut node, 5, &[(WATCHED, b"one")]);
	let (first_at, later_at) = (node.block(5), node.block(6));
	let (node, provider, chain) = serve(node);
	let received = start_watch(&chain, WatchedHead::Best, &[WATCHED, OTHER]);

	let first = next_item_within_timeout(&received).unwrap();
	push_storage_notification(&node, &provider, 6, &[(OTHER, Some(b"two"))]);
	let later = next_item_within_timeout(&received).unwrap();

	assert_eq!((first.at, first.value), (first_at, vec![(0, Some(b"one".to_vec())), (1, None)]));
	assert_eq!((later.at, later.value), (later_at, vec![(1, Some(b"two".to_vec()))]));
	assert_eq!(method_count(&provider, "state_queryStorageAt"), 0);
}

#[test]
fn a_best_watch_drops_keys_a_block_wrote_with_the_value_they_already_had() {
	// A node names every key a block wrote, and several parachain blocks can
	// share one timestamp: an unchanged value is not a change.
	let mut node = node_with_best_block_five();
	store_values_at_block(&mut node, 5, &[(WATCHED, b"one"), (OTHER, b"two")]);
	let at = node.block(7);
	let (node, provider, chain) = serve(node);
	let received = start_watch(&chain, WatchedHead::Best, &[WATCHED, OTHER]);
	next_item_within_timeout(&received).unwrap();

	push_storage_notification(&node, &provider, 6, &[(WATCHED, Some(b"one"))]);
	push_storage_notification(
		&node,
		&provider,
		7,
		&[(WATCHED, Some(b"one")), (OTHER, Some(b"three"))],
	);
	let changes = next_item_within_timeout(&received).unwrap();

	assert_eq!((changes.at, changes.value), (at, vec![(1, Some(b"three".to_vec()))]));
}

#[test]
fn a_best_watch_fetches_metadata_again_only_after_a_runtime_upgrade() {
	let upgrade = last_runtime_upgrade_key();
	let mut node = node_with_best_block_five();
	store_values_at_block(&mut node, 5, &[(&upgrade, b"spec 1")]);
	let (node, provider, chain) = serve(node);
	let received = start_watch(&chain, WatchedHead::Best, &[WATCHED]);
	next_item_within_timeout(&received).unwrap();
	let first = method_count(&provider, "Core_version");
	let fetches_after = |number: u64, changes: &[(&[u8], Option<&[u8]>)]| {
		push_storage_notification(&node, &provider, number, changes);
		next_item_within_timeout(&received).unwrap();
		method_count(&provider, "Core_version")
	};

	let unchanged = fetches_after(6, &[(WATCHED, Some(b"a"))]);
	let upgraded = fetches_after(7, &[(&upgrade, Some(b"spec 2")), (WATCHED, Some(b"b"))]);

	assert_eq!([unchanged, upgraded], [first, first + 1]);
}

#[test]
fn a_best_watch_whose_first_notification_leaves_a_key_out_is_an_error() {
	// Later notifications name only changed keys, so a key missing from the
	// first one would have no value at all rather than an absent one.
	let mut node = Node::new(3, 5);
	node.unanswered_keys.insert(OTHER.to_vec());
	let (_node, _provider, chain) = serve(node);
	let received = start_watch(&chain, WatchedHead::Best, &[WATCHED, OTHER]);

	let error = next_item_within_timeout(&received).unwrap_err();

	let key = hex::encode(OTHER);
	assert!(error.to_string().contains(&format!("key 0x{key} at")), "{error}");
}
