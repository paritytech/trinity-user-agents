//! Stable host-rust-core bridge to the standalone PolkaVM application runtime.
//!
//! Host applications depend on this crate rather than pinning the standalone
//! runtime repository directly. The bridge revision identifies one reviewed
//! native runtime, GPU wire contract, and browser asset set.

pub use pvm_runtime::*;

#[cfg(feature = "browser-assets")]
pub use pvm_runtime_assets::{BrowserAsset, RUNTIME_VERSION, browser_assets};

/// Immutable standalone runtime source consumed by this bridge release.
pub const RUNTIME_SOURCE_REVISION: &str = "1d73f040049583d31998d590a6f29c3c47b31fbb";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_revision_is_immutable() {
        assert_eq!(RUNTIME_SOURCE_REVISION.len(), 40);
        assert!(
            RUNTIME_SOURCE_REVISION
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
    }

    #[cfg(feature = "browser-assets")]
    #[test]
    fn browser_assets_are_complete_and_source_identified() {
        assert_eq!(RUNTIME_VERSION, "0.1.5");
        assert_eq!(browser_assets().len(), 7);
        assert!(
            browser_assets()
                .iter()
                .any(|asset| asset.path == "pvm-browser-runtime.wasm")
        );
    }
}
