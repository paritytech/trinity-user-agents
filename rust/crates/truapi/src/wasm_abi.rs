//! The `truapi` wasm import module a wasm worker and its host share.
//!
//! A guest starts a call through the import named after the method, which
//! returns a call handle. The host answers later by calling the guest's
//! [`ON_EVENT_EXPORT`] once per [`EventKind`] with the payload's length; the
//! guest copies the payload into a buffer of its own through
//! [`READ_EVENT_IMPORT`]. The host never allocates in guest memory.
//!
//! A guest also gets random bytes, the time of two clocks, and timers. A
//! timer has a handle and answers through the same events as a call.
//!
//! The host also starts calls into the guest: the host-initiated methods,
//! such as `renderer_render` and `chat_user_action`. The host calls the
//! guest's [`ON_CALL_EXPORT`] with a call id of its own and the lengths of
//! the method name and the request. The guest copies both through
//! [`READ_EVENT_IMPORT`]. The guest answers through [`CALL_ITEM_IMPORT`] and
//! [`CALL_END_IMPORT`], and [`ON_CANCEL_EXPORT`] tells the guest that the
//! host stopped the call.
//!
//! When the host stops the worker, it calls the guest's [`ON_STOP_EXPORT`]
//! once, if the guest exports it. The host then keeps answering the calls of
//! the guest for a short grace time, so that the guest can write what it
//! holds in memory. The guest ends through [`FINISH_IMPORT`] when it is
//! ready.
//!
//! The host only touches guest memory behind a pointer an import passes, within
//! the length the guest gave or was told, which is why guests declare those
//! imports safe.

/// Import module of every host function.
pub const IMPORT_MODULE: &str = "truapi";

/// Import a guest calls to withdraw a call it no longer awaits.
pub const RELEASE_IMPORT: &str = "release";

/// Import copying the payload of the event being delivered into guest memory,
/// for exactly the length [`ON_EVENT_EXPORT`] announced.
pub const READ_EVENT_IMPORT: &str = "read_event";

/// Import a guest writes a UTF-8 log line through.
pub const LOG_IMPORT: &str = "log";

/// Import a guest reports the end of its entry point through, with a UTF-8
/// error message when it failed.
pub const FINISH_IMPORT: &str = "finish";

/// Import filling `(pointer, len)` of guest memory with random bytes from the
/// host's operating system generator.
pub const RANDOM_IMPORT: &str = "random";

/// Import that returns the Unix time of the host clock in milliseconds.
pub const NOW_MILLIS_IMPORT: &str = "now_millis";

/// Import that returns the milliseconds since the host loaded the worker.
/// The value never goes back.
pub const MONOTONIC_MILLIS_IMPORT: &str = "monotonic_millis";

/// Import that starts a timer of `millis` milliseconds and returns its handle.
/// When the time elapses, the host sends one [`EventKind::Response`] event
/// with an empty payload. [`RELEASE_IMPORT`] cancels the timer.
pub const SLEEP_IMPORT: &str = "sleep";

/// Import a guest sends one item of a call that the host started through:
/// `(call, item_pointer, item_len)`. The item is SCALE in the version of the
/// request.
pub const CALL_ITEM_IMPORT: &str = "call_item";

/// Import a guest ends a call that the host started through:
/// `(call, end_pointer, end_len)`. The end is a SCALE
/// `Result<(), CallError<Error>>` in the version of the request. No item
/// follows it.
pub const CALL_END_IMPORT: &str = "call_end";

/// Export the host calls once to run the guest's entry point.
pub const START_EXPORT: &str = "truapi_start";

/// Export delivering one event: `(handle, kind, payload_len)`.
pub const ON_EVENT_EXPORT: &str = "truapi_on_event";

/// Export starting one call into the guest: `(call, method_len, request_len)`.
/// The payload that [`READ_EVENT_IMPORT`] copies is the UTF-8 method name,
/// such as `chat_user_action`, then the SCALE versioned request. The call id
/// is the host's own, apart from the handles of the guest's calls.
pub const ON_CALL_EXPORT: &str = "truapi_on_call";

/// Export telling the guest that the host stopped a call it started:
/// `(call)`. The host drops each answer that comes after it.
pub const ON_CANCEL_EXPORT: &str = "truapi_on_cancel";

/// Export telling the guest that the host stops it: `()`. It is optional. The
/// host calls it at most once, then answers the calls of the guest until the
/// guest calls [`FINISH_IMPORT`] or a grace time ends. A guest without it
/// ends at once when the host stops it.
pub const ON_STOP_EXPORT: &str = "truapi_on_stop";

/// What one event carries for a call handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum EventKind {
	/// Terminal answer of a request: `Result<Response, CallError<Error>>`.
	Response = 0,
	/// One subscription item.
	Item = 1,
	/// Terminal end of a subscription: `Result<(), CallError<Error>>`.
	End = 2,
}

impl TryFrom<u32> for EventKind {
	type Error = u32;

	fn try_from(kind: u32) -> Result<Self, u32> {
		match kind {
			0 => Ok(Self::Response),
			1 => Ok(Self::Item),
			2 => Ok(Self::End),
			unknown => Err(unknown),
		}
	}
}
