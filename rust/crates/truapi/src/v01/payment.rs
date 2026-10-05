use parity_scale_codec::{Decode, Encode};

use super::account::DerivationIndex;

/// Request to subscribe to payment balance updates.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentBalanceSubscribeRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub purse: Option<u32>,
}

/// Current payment balance state pushed to subscribers.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentBalanceSubscribeItem {
    /// Balance that can be spent right now.
    pub available: u128,
}

/// Source for a payment top-up operation.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum PaymentTopUpSource {
    /// Fund from one of the calling product's scoped accounts.
    ProductAccount {
        /// Account selector within the product subtree; the same selector as
        /// [`ProductAccountId::derivation_index`].
        ///
        /// [`ProductAccountId::derivation_index`]: super::account::ProductAccountId::derivation_index
        derivation_index: DerivationIndex,
    },
    /// Fund from a one-time account represented by its private key. This is a
    /// standard account holding public funds, not a coin key.
    PrivateKey {
        /// Sr25519 secret key bytes.
        sr25519_secret_key: [u8; 64],
    },
    /// Fund directly from coin secret keys. Each key is an sr25519 secret
    /// controlling a single coin.
    Coins {
        /// Sr25519 secret keys, one per coin.
        sr25519_secret_keys: Vec<[u8; 64]>,
    },
}

/// Request to top up the product payment balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostPaymentTopUpRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub into: Option<u32>,
    /// Amount to top up.
    pub amount: u128,
    /// Funding source for the top-up.
    pub source: PaymentTopUpSource,
    /// Caller-chosen id the top-up's status is followed by. Reusing one is
    /// refused with `AlreadyExists`, which makes a retried call safe.
    pub id: [u8; 32],
}

/// Request to initiate a payment to another account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentRequest {
    /// Optional purse selector. `None` means MAIN_PURSE.
    pub from: Option<u32>,
    /// Amount to pay.
    pub amount: u128,
    /// Destination account.
    pub destination: [u8; 32],
}

/// Receipt returned after a successful payment request.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentResponse {
    /// The assigned payment identifier.
    pub id: String,
}

/// Payment lifecycle status pushed to subscribers.
///
/// Once a terminal state (`Completed` or `Failed`) is reached, the host
/// delivers it and may close the subscription.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentStatusSubscribeItem {
    /// Payment is being processed.
    Processing,
    /// Payment has been settled successfully.
    Completed,
    /// Payment has failed.
    Failed {
        /// Failure reason.
        reason: String,
    },
}

/// Error from [`crate::api::Payment::balance_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentBalanceSubscribeError {
    /// User denied the balance disclosure request.
    PermissionDenied,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error from [`crate::api::Payment::top_up`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Error)
)]
pub enum HostPaymentTopUpError {
    /// The source key is malformed, or the source was not found.
    #[display("invalid top-up source")]
    InvalidSource,
    /// A top-up with this id already exists.
    #[display("a top-up with this id already exists")]
    AlreadyExists,
    /// Another top-up from the same source is still running.
    #[display("another top-up from this source is running")]
    SourceBusy,
    /// Catch-all.
    #[display("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error from [`crate::api::Payment::request`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentError {
    /// User rejected the payment request.
    Rejected,
    /// User's available balance is not sufficient for the requested amount.
    InsufficientBalance,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error from [`crate::api::Payment::status_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentStatusSubscribeError {
    /// Payment ID was not found or does not belong to the current product.
    PaymentNotFound,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to subscribe to a payment status.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentStatusSubscribeRequest {
    /// Payment identifier to watch.
    pub payment_id: String,
}

/// Request to follow one top-up.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPaymentTopUpStatusSubscribeRequest {
    /// Id the top-up was started with.
    pub id: [u8; 32],
}

/// Progress of a top-up. `Claimed { finalized: true }`, `ClaimedPartially` and
/// `NotClaimed` are terminal, and stay readable after the top-up ends.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum HostPaymentTopUpStatusSubscribeItem {
    /// Waiting for the source's funds to be seen.
    Detecting,
    /// Claiming the funds into the balance.
    Claiming,
    /// The full amount was claimed.
    Claimed {
        /// Whether the claim is finalized on chain.
        finalized: bool,
    },
    /// Less than the requested amount was claimed, such as when an amount
    /// below the smallest coin denomination remains.
    ClaimedPartially {
        /// Amount actually credited.
        actual_claimed: u128,
    },
    /// Nothing was claimed.
    NotClaimed,
}

/// Error from [`crate::api::Payment::top_up_status_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPaymentTopUpStatusSubscribeError {
    /// No top-up with this id for the calling product.
    NotFound,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
