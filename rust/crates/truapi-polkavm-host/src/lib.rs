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
pub const POLKAVM_HOST_RUNTIME_SOURCE_REVISION: &str = "cc9940b6c53c56e7d8189f7a4a722d6847371c6a";

#[cfg(test)]
mod tests {
    const MANIFEST: &str = include_str!("../Cargo.toml");

    #[test]
    fn test_constants_match_pinned_runtime_dependency() {
        let dependency = MANIFEST
            .lines()
            .find(|line| line.starts_with("polkavm-host-runtime = "))
            .expect("polkavm-host-runtime dependency line");
        assert!(dependency.contains(&format!(
            "rev = \"{}\"",
            super::POLKAVM_HOST_RUNTIME_SOURCE_REVISION
        )));
        assert!(dependency.contains(&format!(
            "version = \"={}\"",
            super::POLKAVM_HOST_RUNTIME_VERSION
        )));
    }
}
