//! Calls a product worker compiled to wasm makes into its host.
//!
//! Each method's import returns a call handle. The host answers through the
//! guest's `truapi_on_event` export, which hands the event to [`deliver`];
//! that wakes the [`Call`] or [`GuestSubscription`] holding the handle.
//! Dropping either before it ends releases the call, which cancels a request
//! and stops a subscription on the host.

use core::cell::RefCell;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::collections::{HashMap, VecDeque};

use futures::Stream;
use parity_scale_codec::{Decode, DecodeAll, Encode};

use crate::CallError;
use crate::versioned::{IntoLatest, Versioned};
use crate::wasm_abi::EventKind;

#[allow(unsafe_code)]
mod imports {
    #[link(wasm_import_module = "truapi")]
    unsafe extern "C" {
        pub safe fn release(handle: u32);
    }
}

/// The import a generated binding starts its call through.
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

fn open<Request: Versioned + Encode>(import: StartImport, request: Request::Latest) -> u32 {
    let payload = Request::wrap_latest(request).encode();
    let handle = import(payload.as_ptr(), payload.len() as u32);
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
    T::decode_all(&mut &payload[..]).map_err(|error| CallError::MalformedFrame {
        reason: error.to_string(),
    })
}

/// A request in flight, resolving to the latest response or error.
pub struct Call<Response, Error> {
    handle: Option<u32>,
    payload: PhantomData<fn() -> (Response, Error)>,
}

impl<Response, Error> Call<Response, Error> {
    /// Start a request through its method's import.
    pub fn start<Request: Versioned + Encode>(
        import: StartImport,
        request: Request::Latest,
    ) -> Self {
        Self {
            handle: Some(open::<Request>(import, request)),
            payload: PhantomData,
        }
    }
}

impl<Response, Error> Future for Call<Response, Error>
where
    Response: Decode + IntoLatest,
    Error: Decode + IntoLatest,
{
    type Output = Result<Response::Latest, CallError<Error::Latest>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let handle = self.handle.expect("call polled after completion");
        let Some((_, payload)) = next_event(handle, context) else {
            return Poll::Pending;
        };
        close(handle);
        self.handle = None;
        let result =
            decode::<Result<Response, CallError<Error>>, Error>(&payload).and_then(|result| result);
        Poll::Ready(
            result
                .map(IntoLatest::into_latest)
                .map_err(|error| error.map_domain(IntoLatest::into_latest)),
        )
    }
}

impl<Response, Error> Drop for Call<Response, Error> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            release(handle);
        }
    }
}

/// A subscription in flight. Yields latest items; an `Err` is the
/// interrupt that ended it.
pub struct GuestSubscription<Item, Error> {
    handle: Option<u32>,
    payload: PhantomData<fn() -> (Item, Error)>,
}

impl<Item, Error> GuestSubscription<Item, Error> {
    /// Start a subscription through its method's import.
    pub fn start<Request: Versioned + Encode>(
        import: StartImport,
        request: Request::Latest,
    ) -> Self {
        Self {
            handle: Some(open::<Request>(import, request)),
            payload: PhantomData,
        }
    }
}

impl<Item, Error> Stream for GuestSubscription<Item, Error>
where
    Item: Decode + IntoLatest,
    Error: Decode + IntoLatest,
{
    type Item = Result<Item::Latest, CallError<Error::Latest>>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Some(handle) = self.handle else {
            return Poll::Ready(None);
        };
        let Some((kind, payload)) = next_event(handle, context) else {
            return Poll::Pending;
        };
        if kind == EventKind::Item {
            let item = decode::<Item, Error>(&payload);
            return Poll::Ready(Some(
                item.map(IntoLatest::into_latest)
                    .map_err(|error| error.map_domain(IntoLatest::into_latest)),
            ));
        }
        close(handle);
        self.handle = None;
        let end = decode::<Result<(), CallError<Error>>, Error>(&payload).and_then(|end| end);
        Poll::Ready(
            end.err()
                .map(|error| Err(error.map_domain(IntoLatest::into_latest))),
        )
    }
}

impl<Item, Error> Drop for GuestSubscription<Item, Error> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            release(handle);
        }
    }
}
