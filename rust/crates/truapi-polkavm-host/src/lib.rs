//! Optional composition of the TrUAPI server and PolkaVM host runtime.
//!
//! The base [`truapi_server`] remains independent of PolkaVM. Native hosts that
//! need both surfaces link this crate, which pins one reviewed runtime revision.

/// The pinned PolkaVM host runtime API.
pub use polkavm_host_runtime;
/// The PolkaVM-independent TrUAPI server API.
pub use truapi_server;

/// Version of this optional composition crate.
pub const TRUAPI_POLKAVM_HOST_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Version of the pinned PolkaVM host runtime.
pub const POLKAVM_HOST_RUNTIME_VERSION: &str = "0.2.0";
/// Immutable source revision of the pinned PolkaVM host runtime.
pub const POLKAVM_HOST_RUNTIME_SOURCE_REVISION: &str = "e56fbe58ff3c6d694b1d86368fe87e085710372c";
