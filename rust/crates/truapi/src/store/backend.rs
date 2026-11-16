//! Where the store's closures run.
//!
//! Each target has one `Backend` with the same four operations. `open`
//! prepares the writer, `with_writer` and `with_reader` each run a closure on
//! a connection, and `close` releases the connections, after which every call
//! fails with `DbError::Closed`.

#[cfg(target_arch = "wasm32")]
mod inline;
#[cfg(not(target_arch = "wasm32"))]
mod threaded;

#[cfg(target_arch = "wasm32")]
pub use inline::Backend;
#[cfg(not(target_arch = "wasm32"))]
pub use threaded::Backend;
