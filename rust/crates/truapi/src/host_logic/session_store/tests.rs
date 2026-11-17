use super::*;
use futures::{FutureExt, StreamExt, executor::block_on};

#[test]
fn notify_broadcasts_to_subscribers() {
	let notifier = SessionStoreChangeNotifier::new();
	let mut first = notifier.subscribe();
	let mut second = notifier.subscribe();

	notifier.notify();

	assert!(block_on(first.next()).is_some());
	assert!(block_on(second.next()).is_some());
}

#[test]
fn dropped_subscriber_is_pruned_on_next_notify() {
	let notifier = SessionStoreChangeNotifier::new();
	let dropped = notifier.subscribe();
	drop(dropped);

	notifier.notify();

	assert_eq!(
		notifier
			.subscribers
			.lock()
			.expect("session-store notifier mutex poisoned")
			.len(),
		0
	);
}

#[test]
fn no_tick_without_notify() {
	let notifier = SessionStoreChangeNotifier::new();
	let mut ticks = notifier.subscribe();

	assert!(ticks.next().now_or_never().is_none());
}
