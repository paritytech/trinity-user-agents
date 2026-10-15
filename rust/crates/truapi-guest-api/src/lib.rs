//! Experimental: TrUAPI for a product worker compiled to wasm.
//!
//! A worker is a `cdylib` built for `wasm32-unknown-unknown`. It depends on
//! this crate as `truapi`, so calls read like the TypeScript client's
//! `truapi.account.getUserId()`:
//!
//! ```toml
//! truapi = { package = "truapi-guest-api", path = "..." }
//! ```
//!
//! Its entry point is an async function marked [`main`]. Each service module
//! below holds one async function per TrUAPI method, taking and returning the
//! latest protocol types. The host runs the worker for one product, so every
//! call is made as that product.
//!
//! The host also calls the worker: it asks for a rendered body, a page of the
//! messages of a room the product keeps, or the answer to what the user did.
//! For each such host-initiated method, the service module has an `on_<method>`
//! function that registers the handler, as the TypeScript client's
//! `truapi.chat.onUserAction(...)` does:
//!
//! ```ignore
//! truapi::chat::on_user_action(|request, sink| async move {
//!     truapi::log!("user action in {}", request.room_id);
//!     sink.send(ProductChatUserActionItem::Acknowledged);
//!     Ok(())
//! });
//! ```
//!
//! The handler runs as a task. It sends items through the [`Sink`], and its
//! result ends the call. When the host stops the call, the task is dropped. A
//! method with no handler ends at once with `Unsupported`.
//!
//! ```ignore
//! #[truapi::main]
//! async fn main() -> Result<(), truapi::Error> {
//!     let user = truapi::account::get_user_id(()).await?;
//!     truapi::log!("user id: {}", user.primary_username);
//!     Ok(())
//! }
//! ```
//!
//! [`spawn`] runs more tasks beside the entry point. [`time`] has the clocks
//! and a timer, [`random`] has random bytes, and [`raw`] makes and answers
//! calls with requests and payloads that are already encoded.
//!
//! The crate is empty on targets other than wasm32.

#![cfg(target_arch = "wasm32")]

#[doc(hidden)]
pub mod exports;

/// The SCALE codec of the requests and payloads, for [`raw`] calls.
pub use parity_scale_codec as codec;
pub use truapi::{
	CallError,
	api::{
		account::guest as account, chain::guest as chain, chat::guest as chat,
		coin_payment::guest as coin_payment, contacts::guest as contacts,
		entropy::guest as entropy, expanded_card::guest as expanded_card, game::guest as game,
		local_storage::guest as local_storage, locale::guest as locale,
		notifications::guest as notifications, payment::guest as payment,
		permissions::guest as permissions, pocket::guest as pocket, preimage::guest as preimage,
		renderer::guest as renderer, resource_allocation::guest as resource_allocation,
		scanner::guest as scanner, signing::guest as signing,
		statement_store::guest as statement_store, system::guest as system, theme::guest as theme,
		worker::guest as worker,
	},
	guest::Sink,
	latest, v01, v02, versioned,
};
pub use truapi_macros::guest_main as main;

#[allow(unsafe_code)]
mod imports {
	#[link(wasm_import_module = "truapi")]
	unsafe extern "C" {
		pub safe fn read_event(payload: *mut u8);
		pub safe fn log(line: *const u8, line_len: u32);
		pub safe fn finish(succeeded: u32, message: *const u8, message_len: u32);
		pub safe fn random(buffer: *mut u8, buffer_len: u32);
		pub safe fn now_millis() -> u64;
		pub safe fn monotonic_millis() -> u64;
	}
}

/// Run `future` as a task beside the entry point.
///
/// A task runs while the entry point waits. When the entry point returns, the
/// worker ends and every task stops, which releases the calls it holds.
pub fn spawn(future: impl core::future::Future<Output = ()> + 'static) {
	exports::spawn(future);
}

/// The clocks and timers of the host.
pub mod time {
	use core::{
		future::Future,
		pin::Pin,
		task::{Context, Poll},
		time::Duration,
	};

	use truapi::guest::RawCall;

	use crate::imports;

	/// The Unix time of the host clock, in milliseconds. It can go back
	/// when the clock of the device changes.
	pub fn now_unix_millis() -> u64 {
		imports::now_millis()
	}

	/// The milliseconds since the host loaded the worker. The value never
	/// goes back, so use it to measure intervals.
	pub fn monotonic_millis() -> u64 {
		imports::monotonic_millis()
	}

	/// Wait for `duration`, rounded up to a full millisecond. Dropping the
	/// future cancels the timer on the host.
	pub fn sleep(duration: Duration) -> Sleep {
		let millis =
			duration.as_millis() + u128::from(!duration.subsec_nanos().is_multiple_of(1_000_000));
		Sleep(truapi::guest::sleep(u64::try_from(millis).unwrap_or(u64::MAX)))
	}

	/// A timer of the host, from [`sleep`]. It holds only the timer handle,
	/// so it is `Send`.
	pub struct Sleep(RawCall);

	impl Future for Sleep {
		type Output = ();

		fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
			Pin::new(&mut self.0).poll(context).map(drop)
		}
	}
}

/// Random bytes from the host.
pub mod random {
	use crate::imports;

	/// Largest part of the buffer that one import call fills.
	const CHUNK: usize = 1 << 20;

	/// Fill `buffer` with random bytes from the operating system generator of
	/// the host. They are good for keys.
	pub fn fill(buffer: &mut [u8]) {
		for chunk in buffer.chunks_mut(CHUNK) {
			imports::random(chunk.as_mut_ptr(), chunk.len() as u32);
		}
	}
}

/// Calls with encoded requests and payloads, for code that does its own
/// SCALE codec.
///
/// A request is a versioned request envelope, for example
/// `HostLocalStorageReadRequest::V1(key).encode()`. A call resolves to the
/// payload of the `Response` event, a SCALE
/// `Result<Response, CallError<Error>>` in the version of the request. A
/// subscription yields [`RawEvent::Item`](raw::RawEvent::Item) with the SCALE item, then one
/// [`RawEvent::End`](raw::RawEvent::End) with a SCALE `Result<(), CallError<Error>>`.
///
/// The import of a method is in the `imports` module of its service, for
/// example `truapi::local_storage::imports::read`. Its name is
/// `<trait>_<method>` in the `truapi` wasm module.
///
/// [`on_call`](raw::on_call) answers the calls of a host-initiated method,
/// named `<trait>_<method>` too, such as `chat_user_action`.
pub mod raw {
	pub use truapi::guest::{RawCall, RawEvent, RawSink, RawSubscription, StartImport};

	/// Start a request through `import` with an encoded versioned request.
	pub fn call(import: StartImport, request: &[u8]) -> RawCall {
		RawCall::start(import, request)
	}

	/// Start a subscription through `import` with an encoded versioned
	/// request.
	pub fn subscribe(import: StartImport, request: &[u8]) -> RawSubscription {
		RawSubscription::start(import, request)
	}

	/// Answer each call of the host-initiated `method`, such as
	/// `chat_user_action`, with `handler`. The handler gets the SCALE
	/// versioned request and the sink of the call. Its result is the end of
	/// the call: a SCALE `Result<(), CallError<Error>>` in the version of the
	/// request.
	pub fn on_call<Handler, Answer>(method: &'static str, handler: Handler)
	where
		Handler: Fn(Vec<u8>, RawSink) -> Answer + 'static,
		Answer: core::future::Future<Output = Vec<u8>> + 'static,
	{
		truapi::guest::on_raw_call(method, handler);
	}
}

/// Error a worker's entry point fails with. Any `Debug` value converts into
/// it, so `?` works on every call.
#[derive(derive_more::Display)]
pub struct Error(String);

impl<T: core::fmt::Debug> From<T> for Error {
	fn from(error: T) -> Self {
		Self(format!("{error:?}"))
	}
}

/// Write a line to the host's log for this worker.
pub fn log(line: &str) {
	imports::log(line.as_ptr(), line.len() as u32);
}

/// Format and write a line to the host's log, like `println!`.
#[macro_export]
macro_rules! log {
    ($($argument:tt)*) => {
        $crate::log(&::std::format!($($argument)*))
    };
}

/// Export the worker's entry points, running `entry` when the host starts it.
/// Emitted by [`main`]; the worker ends when `entry` returns.
#[doc(hidden)]
#[macro_export]
macro_rules! export_entry {
	($entry:path) => {
		#[unsafe(no_mangle)]
		pub extern "C" fn truapi_start() {
			$crate::exports::start($entry());
		}

		#[unsafe(no_mangle)]
		pub extern "C" fn truapi_on_event(handle: u32, kind: u32, len: u32) {
			$crate::exports::on_event(handle, kind, len);
		}

		#[unsafe(no_mangle)]
		pub extern "C" fn truapi_on_call(call: u32, method_len: u32, request_len: u32) {
			$crate::exports::on_call(call, method_len, request_len);
		}

		#[unsafe(no_mangle)]
		pub extern "C" fn truapi_on_cancel(call: u32) {
			$crate::exports::on_cancel(call);
		}
	};
}
