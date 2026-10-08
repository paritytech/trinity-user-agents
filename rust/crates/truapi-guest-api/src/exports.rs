//! What the exports [`main`](crate::main) emits run: a single-task
//! executor that polls the entry point at start and after every event the
//! host delivers, for as long as the entry point keeps waking itself.

use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Waker};
use std::sync::Arc;
use std::task::Wake;

use truapi::guest::deliver;
use truapi::wasm_abi::EventKind;

use crate::{Error, imports};

type Entry = Pin<Box<dyn Future<Output = ()>>>;

thread_local! {
    static ENTRY: RefCell<Option<Entry>> = const { RefCell::new(None) };
}

static WOKEN: AtomicBool = AtomicBool::new(false);

struct EntryWaker;

impl Wake for EntryWaker {
    fn wake(self: Arc<Self>) {
        WOKEN.store(true, Ordering::Relaxed);
    }
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
///
/// # Safety
///
/// `payload` must be a buffer [`alloc`] returned for `len`, not yet passed
/// here.
#[allow(unsafe_code)]
pub unsafe fn on_event(handle: u32, kind: u32, payload: *mut u8, len: u32) {
    let kind = EventKind::try_from(kind).expect("host sent an unknown event kind");
    // SAFETY: the caller guarantees `alloc` returned this buffer for `len`.
    let payload =
        unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(payload, len as usize)) };
    deliver(handle, kind, payload.into_vec());
    poll_entry();
}

fn poll_entry() {
    let waker = Waker::from(Arc::new(EntryWaker));
    ENTRY.with_borrow_mut(|entry| {
        while let Some(future) = entry {
            WOKEN.store(false, Ordering::Relaxed);
            if future
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_ready()
            {
                *entry = None;
            } else if !WOKEN.load(Ordering::Relaxed) {
                return;
            }
        }
    });
}
