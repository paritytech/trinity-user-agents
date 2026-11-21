//! Host-agnostic logic the Rust core owns on behalf of every platform host.
//!
//! Platform callbacks are a syscall layer for OS primitives (modals, native
//! storage, URL handler, notification center). Everything else lives here so
//! iOS, Android, and web hosts share one canonical implementation.

// Links `verifiable` directly, which the browser core loads on demand instead.
#[cfg(not(target_arch = "wasm32"))]
pub mod attestation;
mod contact_substitution;
// Only native hosts open a page from a development server.
#[cfg(not(target_arch = "wasm32"))]
pub mod dev_server;
mod device_key;
pub mod dotns;
pub mod dotns_gateway;
pub mod entropy;
pub mod features;
pub mod funding;
pub mod funding_providers;
pub mod funding_view;
mod keyed_hash;
pub mod product_account;
mod raw_signing;
pub mod root_entropy;
pub mod scanner;
pub mod session;
mod session_store;
pub mod sso;
pub mod statement_store;
pub mod worker;
mod worker_manifest;
pub mod x25519_seal;

pub use contact_substitution::{SubstitutionError, substitute};
pub use keyed_hash::blake2b_256_keyed;
pub use device_key::{read_or_create_device_encryption_secret, store_device_encryption_secret};
pub use raw_signing::{RawPayloadError, raw_payload_bytes};
pub use session_store::SessionStoreChangeNotifier;
pub use worker_manifest::{
	ExecutableManifestError, FundingConfig, FundingRoute, WorkerManifest, check_app_manifest,
};
