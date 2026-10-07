//! Write a TrUAPI Worker product in Rust, compiled to `wasm32-unknown-unknown`
//! and run by a host's wasm sandbox.
//!
//! A product implements [`ProductWorker`] on its own state and names its
//! constructor once with [`export_worker!`]. The library does the rest:
//!
//! - the sandbox ABI: the six exports a host calls and the two imports it
//!   provides, with every pointer handled in one audited module;
//! - the wire: the frame envelope, request ids, versioned envelopes, and the
//!   handshake;
//! - calls to the host through [`Ctx::call`], answered in a later turn;
//! - render streams: which bodies are open, redrawing all of them on
//!   [`Ctx::redraw`], suspend and resume;
//! - user actions in drawn bodies, delivered to [`ProductWorker::action`].
//!
//! ```ignore
//! use truapi_worker::truapi::latest::{HostRendererActionSubscribeItem, RenderContext, RendererNode};
//! use truapi_worker::{Ctx, ProductWorker};
//!
//! #[derive(Default)]
//! struct Counter { count: u32 }
//!
//! impl ProductWorker for Counter {
//!     fn draws(&self, body: &RenderContext) -> bool {
//!         matches!(body, RenderContext::PocketCard { .. })
//!     }
//!     fn render(&self, _body: &RenderContext, cx: &mut Ctx<Self>) -> RendererNode {
//!         tree(self.count, cx.paused())
//!     }
//!     fn action(&mut self, action: &HostRendererActionSubscribeItem, cx: &mut Ctx<Self>) {
//!         if action.action_id == "bump" {
//!             self.count += 1;
//!             cx.redraw();
//!         }
//!     }
//! }
//!
//! truapi_worker::export_worker!(Counter::default());
//! ```
//!
//! # Using it from another repository
//!
//! A worker is its own crate, in any repository, with `crate-type =
//! ["cdylib", "rlib"]`, built with `cargo build --target
//! wasm32-unknown-unknown --release`. The `.wasm` it produces is the worker
//! executable a host fetches; this crate is only a build dependency.
//!
//! Neither this crate nor `truapi` is on crates.io yet, so depend on it by git
//! (`truapi-worker = { git = "<this repository>", rev = "<commit>" }`) or by
//! path. It needs `truapi` without its default features, which is the
//! protocol definitions alone: no generated sources, no host runtime. Pin a
//! revision whose `truapi` speaks the same wire codec version as the hosts
//! the worker runs on; the handshake refuses any other.
//!
//! A worker crate may `deny(unsafe_code)` but not `forbid` it: the exports
//! [`export_worker!`] generates must live in that crate and carry
//! `#[allow(unsafe_code)]`.
//!
//! # The sandbox ABI
//!
//! Exports: `alloc(len: i32) -> i32`, `free(ptr: i32, len: i32)`,
//! `on_start()`, `on_frame(ptr: i32, len: i32)`, `on_suspend()`,
//! `on_resume()`, and the module's `memory`. Imports, from module `host`:
//! `frame_send(ptr: i32, len: i32)` and `log(ptr: i32, len: i32)`.
//!
//! One export call is one turn. The host writes a frame into a buffer it got
//! from `alloc`, calls `on_frame`, and frees the buffer with `free` after it
//! returns; the guest never keeps a host pointer past the call. Everything the
//! guest says in a turn goes through the two imports, frames first, then log
//! lines, and the host copies each before the import returns. The guest never
//! blocks on the host.

mod calls;
mod frame;
mod instance;
pub mod testing;
pub mod wire;

pub use calls::{
    Call, CallFailure, ChatCreateRoom, ChatPostMessage, ChatRegisterBot, Reply, SystemHandshake,
};
pub use frame::Frame;
pub use instance::{Ctx, Instance, ProductWorker};
/// The SCALE codec the protocol types are built on, for the helper types
/// they use, such as `OptionBool` in `ButtonProps`.
pub use parity_scale_codec;
/// The protocol crate whose latest payload types a worker reads and builds.
pub use truapi;

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
// The sandbox boundary is a C ABI over raw guest memory; this module is the
// only place it is touched.
#[allow(unsafe_code)]
#[path = "abi.rs"]
pub mod __abi;

/// Export a worker from the final wasm32 crate: `export_worker!(expr)`, where
/// `expr` builds the [`ProductWorker`] once, on the host's first call.
///
/// Expands, on wasm32 only, to the six `#[unsafe(no_mangle)] extern "C"`
/// exports of the sandbox ABI. `free` and `on_frame` are `unsafe extern`
/// because they take guest pointers from the host. Each is one call into the
/// library's ABI module and carries `#[allow(unsafe_code)]`; the product crate
/// writes no `unsafe` of its own, and may `deny(unsafe_code)` but not
/// `forbid` it. Use it once per final crate: a second use defines every
/// export twice.
#[macro_export]
macro_rules! export_worker {
    ($worker:expr $(,)?) => {
        #[cfg(target_arch = "wasm32")]
        #[allow(unsafe_code)]
        const _: () = {
            fn instance() -> ::std::boxed::Box<dyn $crate::__abi::Turns> {
                ::std::boxed::Box::new($crate::Instance::new($worker))
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn alloc(len: u32) -> *mut u8 {
                $crate::__abi::alloc(len)
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn free(ptr: *mut u8, len: u32) {
                unsafe { $crate::__abi::free(ptr, len) }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn on_start() {
                $crate::__abi::on_start(instance)
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn on_frame(ptr: *const u8, len: u32) {
                unsafe { $crate::__abi::on_frame(instance, ptr, len) }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn on_suspend() {
                $crate::__abi::on_suspend(instance)
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn on_resume() {
                $crate::__abi::on_resume(instance)
            }
        };
    };
}
