//! `ProductSubtreeRequest`s that concurrent callers share.

use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::{
	FutureExt,
	future::{BoxFuture, Shared, WeakShared},
};
use truapi::{CallContext, CancellationToken};

use super::super::{authority::AuthorityError, until_cancelled};

/// The account holder's answer to one `ProductSubtreeRequest`.
pub type ProductSubtreeFetch = BoxFuture<'static, Result<[u8; 32], AuthorityError>>;

/// A `ProductSubtreeRequest` in flight, which concurrent callers join.
pub struct InFlightProductSubtree {
	request: WeakShared<ProductSubtreeFetch>,
	/// Callers still waiting on `request`.
	callers: AtomicUsize,
	/// Cancels `request`. Only the last caller to withdraw triggers it, with
	/// its own reason, so the request unwinds as one with a single caller does.
	cancel: CancellationToken,
}

impl InFlightProductSubtree {
	/// Track a new request, returning its entry and its first caller.
	pub fn start(
		request: ProductSubtreeFetch,
		cancel: CancellationToken,
	) -> (Arc<Self>, ProductSubtreeCaller) {
		let request = request.shared();
		let in_flight = Arc::new(Self {
			request: request.downgrade().expect("an unpolled request is pending"),
			callers: AtomicUsize::new(1),
			cancel,
		});
		(in_flight.clone(), ProductSubtreeCaller { request, in_flight, withdrawn: false })
	}

	/// Join the request as one more caller, while it has not settled and not
	/// every caller has left it.
	pub fn join(self: &Arc<Self>) -> Option<ProductSubtreeCaller> {
		let request = self.request.upgrade().filter(|request| request.peek().is_none())?;
		self.callers
			.try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
				(count > 0).then_some(count + 1)
			})
			.ok()?;
		Some(ProductSubtreeCaller { request, in_flight: self.clone(), withdrawn: false })
	}

	/// Whether any caller still waits on the request.
	pub fn has_callers(&self) -> bool {
		self.callers.load(Ordering::Acquire) > 0
	}

	/// Count one caller out, returning whether it was the last.
	fn leave(&self) -> bool {
		self.callers.fetch_sub(1, Ordering::AcqRel) == 1
	}
}

/// One caller's hold on a shared `ProductSubtreeRequest`.
pub struct ProductSubtreeCaller {
	request: Shared<ProductSubtreeFetch>,
	in_flight: Arc<InFlightProductSubtree>,
	withdrawn: bool,
}

impl ProductSubtreeCaller {
	/// Wait for the request's answer unless `cx` is cancelled first.
	///
	/// A cancelled last caller waits, within its own deadline, for the request
	/// to unwind, so an explicit cancel still withdraws the request from the
	/// Account Holder.
	pub async fn answer(mut self, cx: &CallContext) -> Result<[u8; 32], AuthorityError> {
		let cancelled = match until_cancelled(cx, &mut self.request).await {
			Ok(answer) => return answer,
			Err(cancelled) => cancelled,
		};
		self.withdrawn = true;
		if self.in_flight.leave() {
			let reason = cx.cancel().reason().expect("a cancelled call has a reason");
			self.in_flight.cancel.cancel_with_reason(reason);
			let _unwound = (&mut self.request).await;
		}
		Err(cancelled)
	}
}

impl Drop for ProductSubtreeCaller {
	fn drop(&mut self) {
		if !self.withdrawn {
			self.in_flight.leave();
		}
	}
}

#[cfg(test)]
mod tests;
