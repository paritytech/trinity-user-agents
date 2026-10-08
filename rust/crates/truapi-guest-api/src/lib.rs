//! Experimental: TrUAPI for a product worker compiled to wasm.
//!
//! A worker is a `cdylib` built for `wasm32-unknown-unknown`. Its entry point
//! is an async function named by [`main!`]; each service module below holds
//! one async function per TrUAPI method, taking and returning the latest
//! protocol types. The host runs the worker for one product, so every call is
//! made as that product.
//!
//! ```ignore
//! truapi_guest_api::main!(run);
//!
//! async fn run() -> Result<(), truapi_guest_api::Error> {
//!     let user = truapi_guest_api::account::get_user_id(()).await?;
//!     truapi_guest_api::log!("user id: {}", user.primary_username);
//!     Ok(())
//! }
//! ```
//!
//! The crate is empty on targets other than wasm32.

#![cfg(target_arch = "wasm32")]

#[doc(hidden)]
pub mod exports;

pub use truapi::api::account::guest as account;
pub use truapi::api::chain::guest as chain;
pub use truapi::api::chat::guest as chat;
pub use truapi::api::coin_payment::guest as coin_payment;
pub use truapi::api::contacts::guest as contacts;
pub use truapi::api::entropy::guest as entropy;
pub use truapi::api::game::guest as game;
pub use truapi::api::local_storage::guest as local_storage;
pub use truapi::api::locale::guest as locale;
pub use truapi::api::notifications::guest as notifications;
pub use truapi::api::payment::guest as payment;
pub use truapi::api::permissions::guest as permissions;
pub use truapi::api::pocket::guest as pocket;
pub use truapi::api::preimage::guest as preimage;
pub use truapi::api::renderer::guest as renderer;
pub use truapi::api::resource_allocation::guest as resource_allocation;
pub use truapi::api::scanner::guest as scanner;
pub use truapi::api::signing::guest as signing;
pub use truapi::api::statement_store::guest as statement_store;
pub use truapi::api::system::guest as system;
pub use truapi::api::theme::guest as theme;
pub use truapi::api::worker::guest as worker;
pub use truapi::{CallError, latest};

#[allow(unsafe_code)]
mod imports {
    #[link(wasm_import_module = "truapi")]
    unsafe extern "C" {
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
///
/// `entry` is an `async fn() -> Result<(), Error>`. The worker ends when it
/// returns.
#[macro_export]
macro_rules! main {
    ($entry:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn truapi_start() {
            $crate::exports::start($entry());
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn truapi_alloc(len: u32) -> *mut u8 {
            $crate::exports::alloc(len)
        }

        /// # Safety
        ///
        /// Only the host calls this, with a buffer `truapi_alloc` returned for `len`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn truapi_on_event(
            handle: u32,
            kind: u32,
            payload: *mut u8,
            len: u32,
        ) {
            // SAFETY: forwarded unchanged from the host, under the same contract.
            unsafe { $crate::exports::on_event(handle, kind, payload, len) };
        }
    };
}
