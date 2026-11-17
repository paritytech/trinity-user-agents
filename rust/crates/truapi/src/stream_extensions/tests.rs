use futures::executor::block_on;

use super::*;

/// A product is shown each status once, but every failed read, because a
/// failure says nothing about whether the status changed.
#[test]
fn repeated_values_pass_once_and_every_error_passes() {
	let items = stream::iter([Ok(1), Ok(1), Err("lost"), Err("lost"), Ok(1), Ok(2)]);

	let distinct: Vec<Result<u8, &str>> = block_on(items.distinct_until_changed().collect());

	assert_eq!(distinct, vec![Ok(1), Err("lost"), Err("lost"), Ok(2)]);
}

/// The stream ends with the item that marks the end, so a caller sees it.
#[test]
fn items_stop_after_the_first_last_one() {
	let items = stream::iter([1, 2, 3, 4]);

	let taken: Vec<u8> = block_on(items.take_until_inclusive(|item| *item == 2).collect());

	assert_eq!(taken, vec![1, 2]);
}
