use super::*;

// A product that stops following a key the host never writes again must not
// leave its sender on the bus, or subscribe/drop cycles grow it without bound.
#[test]
fn a_dropped_preimage_subscription_is_forgotten_on_the_next_one() {
	let events = NativeEventBus::default();
	drop(events.subscribe_preimage_changes(vec![1]));
	let _live = events.subscribe_preimage_changes(vec![2]);

	assert_eq!(events.preimage_changes.lock().unwrap().len(), 1);
}

#[test]
fn a_dropped_storage_subscription_is_forgotten_on_the_next_one() {
	let events = NativeEventBus::default();
	drop(events.subscribe_storage_changes("dropped".to_string()));
	let _live = events.subscribe_storage_changes("live".to_string());

	assert_eq!(events.storage_changes.lock().unwrap().len(), 1);
}
