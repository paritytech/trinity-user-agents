use super::*;
use futures::{StreamExt, executor::block_on};

fn channel() -> ActionChannel<String> {
	ActionChannel::new("closed")
}

#[test]
fn buffered_actions_are_drained_in_fifo_order() {
	let channel = channel();
	channel.publish("first".to_string()).unwrap();
	channel.publish("second".to_string()).unwrap();

	let mut items = channel.subscribe::<truapi::latest::GenericError>();
	assert_eq!(block_on(items.next()), Some(Ok("first".to_string())));
	assert_eq!(block_on(items.next()), Some(Ok("second".to_string())));
}

#[test]
fn a_refused_action_is_not_a_tap() {
	// A Worker may scan only after a tap it can receive.
	let channel = channel();
	channel.close();

	assert!(channel.publish("tap".to_string()).is_err());
	assert!(!channel.published_within(5));
}

#[test]
fn full_startup_action_buffer_is_reported() {
	let channel = channel();
	for index in 0..ACTION_BUFFER_CAPACITY {
		channel.publish(index.to_string()).unwrap();
	}

	assert!(matches!(
		channel.publish("overflow".to_string()),
		Err(ProductRuntimeError::BufferFull)
	));
}

#[test]
fn detach_keeps_buffered_actions_for_the_next_subscriber() {
	let channel = channel();
	let mut first = channel.subscribe::<truapi::latest::GenericError>();
	channel.publish("live".to_string()).unwrap();
	assert_eq!(block_on(first.next()), Some(Ok("live".to_string())));

	channel.detach();
	assert_eq!(block_on(first.next()), None);
	channel.publish("buffered".to_string()).unwrap();

	let mut second = channel.subscribe::<truapi::latest::GenericError>();
	assert_eq!(block_on(second.next()), Some(Ok("buffered".to_string())));
}

#[test]
fn closing_discards_buffered_actions() {
	let channel = channel();
	channel.publish("discard me".to_string()).unwrap();
	channel.close();

	// Subscribing to a closed channel interrupts, so a product can
	// tell it from a stream that ran and finished.
	let mut items = channel.subscribe::<truapi::latest::GenericError>();
	match block_on(items.next()) {
		Some(Err(CallError::HostFailure { reason })) => assert_eq!(reason, "closed"),
		other => panic!("a closed channel must interrupt, got {other:?}"),
	}
	assert!(matches!(channel.publish("too late".to_string()), Err(ProductRuntimeError::Closed)));
}

#[test]
fn separate_connections_cannot_observe_each_others_actions() {
	let first = channel();
	let second = channel();
	let mut first_items = first.subscribe::<truapi::latest::GenericError>();
	let mut second_items = second.subscribe::<truapi::latest::GenericError>();

	first.publish("first only".to_string()).unwrap();
	second.publish("second only".to_string()).unwrap();
	assert_eq!(block_on(first_items.next()), Some(Ok("first only".to_string())));
	assert_eq!(block_on(second_items.next()), Some(Ok("second only".to_string())));

	second.close();
	assert!(matches!(second.publish("closed".to_string()), Err(ProductRuntimeError::Closed)));
}

#[test]
fn every_open_subscription_of_a_connection_hears_each_action() {
	// An SDK keeps one stream per surface it draws (chat messages, pocket cards, a
	// room footer); a later subscription must not cut an earlier one off.
	let channel = channel();
	let mut messages = channel.subscribe::<truapi::latest::GenericError>();
	let mut footer = channel.subscribe::<truapi::latest::GenericError>();

	channel.publish("press".to_string()).unwrap();
	assert_eq!(block_on(messages.next()), Some(Ok("press".to_string())));
	assert_eq!(block_on(footer.next()), Some(Ok("press".to_string())));

	// A subscription the product dropped is skipped; the others still hear.
	drop(messages);
	channel.publish("again".to_string()).unwrap();
	assert_eq!(block_on(footer.next()), Some(Ok("again".to_string())));

	// With every subscription gone, an action waits for the next one.
	drop(footer);
	channel.publish("later".to_string()).unwrap();
	let mut next = channel.subscribe::<truapi::latest::GenericError>();
	assert_eq!(block_on(next.next()), Some(Ok("later".to_string())));
}
