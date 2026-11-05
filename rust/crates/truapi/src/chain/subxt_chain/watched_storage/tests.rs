use futures::{StreamExt, executor::block_on, stream};

use super::consume_latest;

#[test]
fn heads_waiting_together_collapse_into_the_newest() {
	// A slow read must not leave the watch reading one stale head after
	// another: whatever queued up meanwhile is read once, at its newest.
	let waiting = stream::iter([1, 2, 3]).boxed();

	let read: Vec<u32> = block_on(consume_latest(waiting).collect());

	assert_eq!(read, vec![3]);
}
