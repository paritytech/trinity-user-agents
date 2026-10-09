use alloc::string::String;
use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Request to read a local storage value.
///
/// Storage is private by default: `product: None` addresses the caller's own
/// storage, which is what every v0.1 read resolved to. Naming another product
/// reads that product's storage instead, and succeeds only if that product's
/// manifest grants this caller the `storage` scope.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageReadRequest {
    /// Product whose storage is read. `None`, or the caller's own id, means the
    /// caller, and consults no grant.
    pub product: Option<String>,
    /// Storage key to read.
    pub key: String,
}

/// Local storage read failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
pub enum HostLocalStorageReadError {
    /// Storage quota exceeded.
    #[display("storage quota exhausted")]
    Full,
    /// The addressed storage belongs to another product that has not granted
    /// this caller the `storage` scope.
    ///
    /// One variant answers every reason: the product does not resolve, it
    /// published no manifest, or its manifest grants this caller nothing.
    /// Distinguishing them would make the call a probe for which products exist
    /// and which hold data.
    #[display("the owning product grants no read access to its storage")]
    AccessNotGranted,
    /// Catch-all.
    #[display("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
