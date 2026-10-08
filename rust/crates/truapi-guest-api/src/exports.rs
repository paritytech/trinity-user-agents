//! What the exports [`main!`](crate::main) defines run: a single-task
//! executor that polls the entry point once at start and again after every
//! event the host delivers.

use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use truapi::guest::deliver;
use truapi::wasm_abi::EventKind;

use crate::{Error, imports};

type Entry = Pin<Box<dyn Future<Output = ()>>>;

thread_local! {
    static ENTRY: RefCell<Option<Entry>> = const { RefCell::new(None) };
}

/// Run `entry` until its first pending call, reporting its outcome to the
/// host when it returns.
pub fn start(entry: impl Future<Output = Result<(), Error>> + 'static) {
    let entry = async move {
        match entry.await {
            Ok(()) => imports::finish(1, core::ptr::null(), 0),
            Err(Error(message)) => imports::finish(0, message.as_ptr(), message.len() as u32),
        }
    };
    ENTRY.set(Some(Box::pin(entry)));
    poll_entry();
}

/// Memory for a payload the host is about to write. Ownership passes back
/// with the [`on_event`] call that names it.
pub fn alloc(len: u32) -> *mut u8 {
    Box::into_raw(vec![0u8; len as usize].into_boxed_slice()).cast()
}

/// Hand one host event to the call it belongs to, then resume the entry
/// point.
pub fn on_event(handle: u32, kind: u32, payload: *mut u8, len: u32) {
    let kind = EventKind::try_from(kind).expect("host sent an unknown event kind");
    #[allow(unsafe_code)]
    // SAFETY: the host passes back, unchanged, a buffer `alloc` returned for `len`.
    let payload =
        unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(payload, len as usize)) };
    deliver(handle, kind, payload.into_vec());
    poll_entry();
}

fn poll_entry() {
    ENTRY.with_borrow_mut(|entry| {
        let Some(future) = entry else {
            return;
        };
        if let Poll::Ready(()) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            *entry = None;
        }
    });
}
