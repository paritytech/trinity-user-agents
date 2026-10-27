//! Stream combinators that `futures` lacks, called like its own.

use core::future::ready;

use futures::stream::{self, BoxStream, Stream, StreamExt};

/// Combinators for every stream that can be boxed.
pub trait StreamExtensions: Stream + Sized + Send + 'static {
	/// The items up to and including the first one `is_last` picks, then
	/// nothing.
	fn take_until_inclusive(
		self,
		is_last: impl Fn(&Self::Item) -> bool + Send + 'static,
	) -> BoxStream<'static, Self::Item>
	where
		Self::Item: Send,
	{
		stream::unfold(Some((self.boxed(), is_last)), |state| async move {
			let (mut items, is_last) = state?;
			let item = items.next().await?;
			let rest = (!is_last(&item)).then_some((items, is_last));
			Some((item, rest))
		})
		.boxed()
	}

	/// The items without each `Ok` value equal to the last `Ok` value passed
	/// on. Every `Err` passes on, and does not count as a value.
	#[cfg(not(target_arch = "wasm32"))]
	fn distinct_until_changed<T, E>(self) -> BoxStream<'static, Result<T, E>>
	where
		Self: Stream<Item = Result<T, E>>,
		T: PartialEq + Clone + Send + 'static,
		E: Send + 'static,
	{
		let mut last = None;
		let is_new = move |item: &Result<T, E>| {
			let Ok(value) = item else { return ready(true) };
			let previous = last.replace(value.clone());
			ready(previous.as_ref() != Some(value))
		};
		self.filter(is_new).boxed()
	}
}

impl<S: Stream + Send + 'static> StreamExtensions for S {}

#[cfg(test)]
mod tests {
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
}
