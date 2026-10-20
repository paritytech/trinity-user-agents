//! [`StorageSnapshots`] for `ChainRuntime`, against the scripted node.

use std::sync::{
	Arc, Mutex,
	mpsc::{Receiver, channel},
};

use futures::{StreamExt, executor::block_on, stream::BoxStream};
use sp_crypto_hashing::twox_128;

use super::{
	BEST_SUBSCRIPTION, FINALIZED_SUBSCRIPTION, GENESIS, Node, TIMEOUT, head_notification,
	method_count, serve, wait_for_subscriptions,
};
use crate::{
	chain::{StorageSnapshot, StorageSnapshots, WatchedHead},
	chain_runtime::{ChainRuntime, RuntimeFailure},
	test_support::{ScriptedProvider, notification_sender},
};

const WATCHED: &[u8] = b"watched";
const OTHER: &[u8] = b"other";

type Snapshots = Receiver<Result<StorageSnapshot, RuntimeFailure>>;

fn last_runtime_upgrade_key() -> Vec<u8> {
	[twox_128(b"System"), twox_128(b"LastRuntimeUpgrade")].concat()
}

/// Polls `snapshots` on a thread, since the node only announces heads after
/// the subscriptions are open, and hands each one over as it arrives.
fn forward_in_background(
	mut snapshots: BoxStream<'static, Result<StorageSnapshot, RuntimeFailure>>,
) -> Snapshots {
	let (sender, receiver) = channel();
	std::thread::spawn(move || {
		block_on(async move {
			while let Some(snapshot) = snapshots.next().await {
				if sender.send(snapshot).is_err() {
					return;
				}
			}
		})
	});
	receiver
}

/// Starts a watch of `keys` at `head` and waits until its head
/// subscriptions are open.
fn watch(
	provider: &ScriptedProvider,
	chain: &ChainRuntime,
	head: WatchedHead,
	keys: &[&[u8]],
) -> Snapshots {
	let keys = keys.iter().map(|key| key.to_vec()).collect();
	let snapshots = block_on(chain.snapshots(GENESIS, head, keys)).unwrap();
	let received = forward_in_background(snapshots);
	wait_for_subscriptions(provider);
	received
}

fn announce(node: &Arc<Mutex<Node>>, provider: &ScriptedProvider, subscription: &str, number: u64) {
	let header = node.lock().unwrap().headers[number as usize].clone();
	notification_sender(provider)
		.unbounded_send(head_notification(subscription, &header))
		.unwrap();
}

fn store(node: &mut Node, number: u64, entries: &[(&[u8], &[u8])]) {
	let at = node.block(number).hash;
	let entries = entries.iter().map(|(key, value)| (key.to_vec(), value.to_vec()));
	node.storage.entry(at).or_default().extend(entries);
}

#[test]
fn each_finalized_head_reads_every_watched_key_in_one_call() {
	let mut node = Node::new(3, 5);
	store(&mut node, 4, &[(WATCHED, b"one")]);
	let at = node.block(4);
	let (node, provider, chain) = serve(node);
	let received = watch(&provider, &chain, WatchedHead::Finalized, &[WATCHED, OTHER]);

	announce(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	let snapshot = received.recv_timeout(TIMEOUT).expect("a snapshot").unwrap();

	assert_eq!((snapshot.at, snapshot.values), (at, vec![Some(b"one".to_vec()), None]));
	assert_eq!(method_count(&provider, "state_queryStorageAt"), 1);
}

#[test]
fn a_best_watch_reads_at_best_heads_only() {
	let mut node = Node::new(3, 5);
	store(&mut node, 4, &[(WATCHED, b"finalized")]);
	store(&mut node, 5, &[(WATCHED, b"best")]);
	let at = node.block(5);
	let (node, provider, chain) = serve(node);
	let received = watch(&provider, &chain, WatchedHead::Best, &[WATCHED]);

	announce(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	announce(&node, &provider, BEST_SUBSCRIPTION, 5);
	let snapshot = received.recv_timeout(TIMEOUT).expect("a snapshot").unwrap();

	assert_eq!((snapshot.at, snapshot.values), (at, vec![Some(b"best".to_vec())]));
}

#[test]
fn a_finalized_watch_does_not_read_at_best_heads() {
	let mut node = Node::new(3, 5);
	store(&mut node, 4, &[(WATCHED, b"finalized")]);
	store(&mut node, 5, &[(WATCHED, b"best")]);
	let at = node.block(4);
	let (node, provider, chain) = serve(node);
	let received = watch(&provider, &chain, WatchedHead::Finalized, &[WATCHED]);

	announce(&node, &provider, BEST_SUBSCRIPTION, 5);
	// Long enough for a wrongly accepted best head to be read before the
	// finalized one arrives, so the two cannot collapse into one read.
	std::thread::sleep(core::time::Duration::from_millis(200));
	announce(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	let snapshot = received.recv_timeout(TIMEOUT).expect("a snapshot").unwrap();

	assert_eq!((snapshot.at, snapshot.values), (at, vec![Some(b"finalized".to_vec())]));
}

#[test]
fn metadata_is_fetched_again_only_after_a_runtime_upgrade() {
	// Every metadata fetch at a block starts with a `Core_version` call, so
	// counting them counts the fetches.
	let upgrade = last_runtime_upgrade_key();
	let mut node = Node::new(3, 7);
	store(&mut node, 4, &[(&upgrade, b"spec 1")]);
	store(&mut node, 5, &[(&upgrade, b"spec 1")]);
	store(&mut node, 6, &[(&upgrade, b"spec 2")]);
	let (node, provider, chain) = serve(node);
	let received = watch(&provider, &chain, WatchedHead::Finalized, &[WATCHED]);
	let fetches_after = |number: u64| {
		announce(&node, &provider, FINALIZED_SUBSCRIPTION, number);
		received.recv_timeout(TIMEOUT).expect("a snapshot").unwrap();
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
	let received = watch(&provider, &chain, WatchedHead::Finalized, &[WATCHED, OTHER]);

	announce(&node, &provider, FINALIZED_SUBSCRIPTION, 4);
	let error = received.recv_timeout(TIMEOUT).expect("a snapshot").unwrap_err();

	let key = hex::encode(OTHER);
	assert!(error.to_string().contains(&format!("key 0x{key} at")), "{error}");
}
