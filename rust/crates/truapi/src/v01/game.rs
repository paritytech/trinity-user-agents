use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Request to remind the user when this product's next game starts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRemindNextGameRequest {
    /// Milliseconds since the Unix epoch, UTC, at which the game starts.
    pub starts_at: u64,
}

/// Why a reminder was not taken.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Error)
)]
pub enum HostRemindNextGameError {
    /// `starts_at` is not after the device's current time.
    #[display("the game has already started")]
    StartsInPast,
}

/// Request to drop this product's reminder.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCancelNextGameRequest {}
