use super::*;

// Registering forgets the senders of streams already dropped, so
// subscribe/drop cycles on an unchanged value never pile up dead senders.
#[test]
fn a_dropped_stream_is_forgotten_on_the_next_registration() {
	let mut subscribers = Subscribers::<(), u8>::default();
	for _ in 0..5 {
		let (sender, receiver) = mpsc::unbounded();
		subscribers.register((), sender);
		drop(receiver);
	}
	let (sender, _live) = mpsc::unbounded();
	subscribers.register((), sender);

	assert_eq!(subscribers.len(), 1);
}
