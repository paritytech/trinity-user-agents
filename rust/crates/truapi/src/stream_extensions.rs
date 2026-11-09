//! Stream combinators that `futures` lacks, called like its own.

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
		use core::future::ready;

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
mod tests;
