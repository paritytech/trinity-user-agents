//! Calls a product worker compiled to wasm makes into its host.
//!
//! Each method's import returns a call handle. The host answers through the
//! guest's `truapi_on_event` export, which hands the event to [`deliver`];
//! that wakes the [`Call`] or [`GuestSubscription`] holding the handle.
//! Dropping either before it ends releases the call, which cancels a request
//! and stops a subscription on the host.
//!
//! [`RawCall`] and [`RawSubscription`] are the same calls without the codec:
//! they take an encoded versioned request and give the encoded payloads. The
//! typed calls use them. A timer from [`sleep`] is a [`RawCall`] too.

use core::{
	cell::RefCell,
	future::Future,
	marker::PhantomData,
	pin::Pin,
	task::{Context, Poll, Waker},
};
use std::collections::{HashMap, VecDeque};

use futures::Stream;
use parity_scale_codec::{Decode, DecodeAll, Encode};

use crate::{
	CallError,
	versioned::{IntoLatest, Versioned},
	wasm_abi::EventKind,
};

#[allow(unsafe_code)]
mod imports {
	#[link(wasm_import_module = "truapi")]
	unsafe extern "C" {
		pub safe fn release(handle: u32);
		pub safe fn sleep(millis: u64) -> u32;
	}
}

/// The import that starts a call of one method. The guest bindings declare
/// one for each method, under the name `<trait>_<method>`.
pub type StartImport = extern "C" fn(request: *const u8, request_len: u32) -> u32;

#[derive(Default)]
struct Slot {
	events: VecDeque<(EventKind, Vec<u8>)>,
	waker: Option<Waker>,
}

thread_local! {
	static CALLS: RefCell<HashMap<u32, Slot>> = RefCell::new(HashMap::new());
}

/// Queue an event the host delivered for `handle`. Events for a call the
/// guest already released are dropped.
pub fn deliver(handle: u32, kind: EventKind, payload: Vec<u8>) {
	let waker = CALLS.with_borrow_mut(|calls| {
		let slot = calls.get_mut(&handle)?;
		slot.events.push_back((kind, payload));
		slot.waker.take()
	});
	if let Some(waker) = waker {
		waker.wake();
	}
}

/// Make a slot for the events of a handle that the host gave.
fn adopt(handle: u32) -> u32 {
	CALLS.with_borrow_mut(|calls| calls.insert(handle, Slot::default()));
	handle
}

fn next_event(handle: u32, context: &Context<'_>) -> Option<(EventKind, Vec<u8>)> {
	CALLS.with_borrow_mut(|calls| {
		let slot = calls.get_mut(&handle)?;
		let event = slot.events.pop_front();
		if event.is_none() {
			slot.waker = Some(context.waker().clone());
		}
		event
	})
}

fn close(handle: u32) {
	CALLS.with_borrow_mut(|calls| calls.remove(&handle));
}

fn release(handle: u32) {
	close(handle);
	imports::release(handle);
}

fn decode<T: Decode, E>(payload: &[u8]) -> Result<T, CallError<E>> {
	T::decode_all(&mut &payload[..])
		.map_err(|error| CallError::MalformedFrame { reason: error.to_string() })
}

/// A request in flight, with no codec. It resolves to the payload of the
/// `Response` event: a SCALE `Result<Response, CallError<Error>>` in the
/// version of the request. Dropping it before it resolves releases the call.
///
/// It holds only the handle, so it is `Send` and can be in an `async_trait`
/// future.
pub struct RawCall {
	handle: Option<u32>,
}

impl RawCall {
	/// Start a request through `import` with an encoded versioned request.
	pub fn start(import: StartImport, request: &[u8]) -> Self {
		let handle = import(request.as_ptr(), request.len() as u32);
		Self { handle: Some(adopt(handle)) }
	}
}

impl Future for RawCall {
	type Output = Vec<u8>;

	fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Vec<u8>> {
		let handle = self.handle.expect("call polled after completion");
		let Some((_, payload)) = next_event(handle, context) else {
			return Poll::Pending;
		};
		close(handle);
		self.handle = None;
		Poll::Ready(payload)
	}
}

impl Drop for RawCall {
	fn drop(&mut self) {
		if let Some(handle) = self.handle {
			release(handle);
		}
	}
}

/// Start a timer of `millis` milliseconds. The call resolves to an empty
/// payload when the time elapses. Dropping it before then cancels the timer.
pub fn sleep(millis: u64) -> RawCall {
	RawCall { handle: Some(adopt(imports::sleep(millis))) }
}

/// One event of a [`RawSubscription`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawEvent {
	/// One item: the SCALE item in the version of the request.
	Item(Vec<u8>),
	/// The end: a SCALE `Result<(), CallError<Error>>`. No event follows it.
	End(Vec<u8>),
}

/// A subscription in flight, with no codec. It yields [`RawEvent::Item`]
/// zero or more times, then one [`RawEvent::End`], then nothing. Dropping it
/// before the end releases the call, which stops the subscription on the
/// host.
///
/// It holds only the handle, so it is `Send` and can be in an `async_trait`
/// future.
pub struct RawSubscription {
	handle: Option<u32>,
}

impl RawSubscription {
	/// Start a subscription through `import` with an encoded versioned
	/// request.
	pub fn start(import: StartImport, request: &[u8]) -> Self {
		let handle = import(request.as_ptr(), request.len() as u32);
		Self { handle: Some(adopt(handle)) }
	}
}

impl Stream for RawSubscription {
	type Item = RawEvent;

	fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<RawEvent>> {
		let Some(handle) = self.handle else {
			return Poll::Ready(None);
		};
		let Some((kind, payload)) = next_event(handle, context) else {
			return Poll::Pending;
		};
		if kind == EventKind::Item {
			return Poll::Ready(Some(RawEvent::Item(payload)));
		}
		close(handle);
		self.handle = None;
		Poll::Ready(Some(RawEvent::End(payload)))
	}
}

impl Drop for RawSubscription {
	fn drop(&mut self) {
		if let Some(handle) = self.handle {
			release(handle);
		}
	}
}

/// A request in flight, resolving to the latest response or error.
pub struct Call<Response, Error> {
	raw: RawCall,
	payload: PhantomData<fn() -> (Response, Error)>,
}

impl<Response, Error> Call<Response, Error> {
	/// Start a request through its method's import.
	pub fn start<Request: Versioned + Encode>(
		import: StartImport,
		request: Request::Latest,
	) -> Self {
		let request = Request::wrap_latest(request).encode();
		Self { raw: RawCall::start(import, &request), payload: PhantomData }
	}
}

impl<Response, Error> Future for Call<Response, Error>
where
	Response: Decode + IntoLatest,
	Error: Decode + IntoLatest,
{
	type Output = Result<Response::Latest, CallError<Error::Latest>>;

	fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
		let Poll::Ready(payload) = Pin::new(&mut self.raw).poll(context) else {
			return Poll::Pending;
		};
		let result =
			decode::<Result<Response, CallError<Error>>, Error>(&payload).and_then(|result| result);
		Poll::Ready(
			result
				.map(IntoLatest::into_latest)
				.map_err(|error| error.map_domain(IntoLatest::into_latest)),
		)
	}
}

/// A subscription in flight. Yields latest items; an `Err` is the
/// interrupt that ended it.
pub struct GuestSubscription<Item, Error> {
	raw: RawSubscription,
	payload: PhantomData<fn() -> (Item, Error)>,
}

impl<Item, Error> GuestSubscription<Item, Error> {
	/// Start a subscription through its method's import.
	pub fn start<Request: Versioned + Encode>(
		import: StartImport,
		request: Request::Latest,
	) -> Self {
		let request = Request::wrap_latest(request).encode();
		Self { raw: RawSubscription::start(import, &request), payload: PhantomData }
	}
}

impl<Item, Error> Stream for GuestSubscription<Item, Error>
where
	Item: Decode + IntoLatest,
	Error: Decode + IntoLatest,
{
	type Item = Result<Item::Latest, CallError<Error::Latest>>;

	fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let event = match Pin::new(&mut self.raw).poll_next(context) {
			Poll::Pending => return Poll::Pending,
			Poll::Ready(None) => return Poll::Ready(None),
			Poll::Ready(Some(event)) => event,
		};
		match event {
			RawEvent::Item(payload) => {
				let item = decode::<Item, Error>(&payload);
				Poll::Ready(Some(
					item.map(IntoLatest::into_latest)
						.map_err(|error| error.map_domain(IntoLatest::into_latest)),
				))
			},
			RawEvent::End(payload) => {
				let end =
					decode::<Result<(), CallError<Error>>, Error>(&payload).and_then(|end| end);
				Poll::Ready(end.err().map(|error| Err(error.map_domain(IntoLatest::into_latest))))
			},
		}
	}
}
