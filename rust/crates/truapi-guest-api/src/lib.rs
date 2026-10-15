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
//! ```ignore
//! #[truapi::main]
//! async fn main() -> Result<(), truapi::Error> {
//!     let user = truapi::account::get_user_id(()).await?;
//!     truapi::log!("user id: {}", user.primary_username);
//!     Ok(())
//! }
//! ```
//!
//! The crate is empty on targets other than wasm32.

#![cfg(target_arch = "wasm32")]

#[doc(hidden)]
pub mod exports;

pub use truapi::{
	CallError,
	api::{
		account::guest as account, chain::guest as chain, chat::guest as chat,
		coin_payment::guest as coin_payment, contacts::guest as contacts,
		entropy::guest as entropy, game::guest as game, local_storage::guest as local_storage,
		locale::guest as locale, notifications::guest as notifications, payment::guest as payment,
		permissions::guest as permissions, pocket::guest as pocket, preimage::guest as preimage,
		renderer::guest as renderer, resource_allocation::guest as resource_allocation,
		scanner::guest as scanner, signing::guest as signing,
		statement_store::guest as statement_store, system::guest as system, theme::guest as theme,
		worker::guest as worker,
	},
	latest,
};
pub use truapi_macros::guest_main as main;

#[allow(unsafe_code)]
mod imports {
	#[link(wasm_import_module = "truapi")]
	unsafe extern "C" {
		pub safe fn read_event(payload: *mut u8);
		pub safe fn log(line: *const u8, line_len: u32);
		pub safe fn finish(succeeded: u32, message: *const u8, message_len: u32);
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
	};
}
