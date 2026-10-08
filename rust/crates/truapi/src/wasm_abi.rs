//! The `truapi` wasm import module a wasm worker and its host share.
//!
//! A guest starts a call through the import named after the method, which
//! returns a call handle. The host answers later by calling the guest's
//! [`ON_EVENT_EXPORT`] once per [`EventKind`] with the payload's length; the
//! guest copies the payload into a buffer of its own through
//! [`READ_EVENT_IMPORT`]. The host never allocates in guest memory.
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

/// Export the host calls once to run the guest's entry point.
pub const START_EXPORT: &str = "truapi_start";

/// Export delivering one event: `(handle, kind, payload_len)`.
pub const ON_EVENT_EXPORT: &str = "truapi_on_event";

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
