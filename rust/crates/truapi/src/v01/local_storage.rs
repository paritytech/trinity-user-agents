use alloc::{string::String, vec::Vec};
use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Request to write a value into local storage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageWriteRequest {
    /// Storage key to write.
    pub key: String,
    /// Value to store at the key.
    pub value: Vec<u8>,
}

/// Local storage operation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Error)
)]
pub enum HostLocalStorageReadError {
    /// Storage quota exceeded.
    #[display("storage quota exhausted")]
    Full,
    /// Catch-all.
    #[display("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to read a local storage value.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageReadRequest {
    /// Storage key to read.
    pub key: String,
}

/// Response containing an optional local storage value.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageReadResponse {
    /// Stored value, if present.
    pub value: Option<Vec<u8>>,
}

/// Request to clear a local storage key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageClearRequest {
    /// Storage key to clear.
    pub key: String,
}

/// Request to subscribe to changes of one local storage key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostLocalStorageSubscribeRequest {
    /// Storage key to observe.
    pub key: String,
}

/// A change to a subscribed storage key, pushed to the subscriber.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostLocalStorageChangeItem {
    /// Value after the change. `Some` on write, `None` after clear.
    pub value: Option<Vec<u8>>,
}
