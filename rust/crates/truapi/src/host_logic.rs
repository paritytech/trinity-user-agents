//! Host-agnostic logic the Rust core owns on behalf of every platform host.
//!
//! Platform callbacks are a syscall layer for OS primitives (modals, native
//! storage, URL handler, notification center). Everything else lives here so
//! iOS, Android, and web hosts share one canonical implementation.

// Links `verifiable` directly, which the browser core loads on demand instead.
#[cfg(not(target_arch = "wasm32"))]
pub mod attestation;
pub mod contact_substitution;
pub mod device_key;
pub mod dotns;
pub mod dotns_gateway;
pub mod entropy;
pub mod features;
pub mod funding;
pub mod product_account;
pub mod raw_signing;
pub mod session;
pub mod session_store;
pub mod sso;
pub mod statement_store;
pub mod worker;
