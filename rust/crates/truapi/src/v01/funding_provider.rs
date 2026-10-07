use parity_scale_codec::{Decode, Encode};

use super::funding::{FundingDirection, FundingFailure};

/// A funding session as the provider serving it receives it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FundingAssignment {
    /// Session id, used in every report about it.
    pub intent: String,
    /// Which way value moves.
    pub direction: FundingDirection,
    /// Amount sought, in the user's payment balance units. `None` when the
    /// user left it to the provider's screens.
    pub amount: Option<u128>,
    /// When the session expires unless funds are already moving, in Unix
    /// milliseconds.
    pub expires_at: u64,
    /// The last update the provider reported, so a restarted worker resumes
    /// where it stopped.
    pub last_update: Option<FundingUpdate>,
}

/// Progress a provider reports for a session it serves.
///
/// Updates only move forward. Once funds are moving the host decides the
/// outcome: an inbound session is delivered from the claims of the top-ups it
/// names, an outbound one is released once the user's payment completes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingUpdate {
    /// Inbound: waiting for the user to pay the provider.
    AwaitingPayment,
    /// Inbound: the provider has seen the user's payment.
    PaymentReceived {
        /// Whether the payment can no longer be reversed.
        finalized: bool,
    },
    /// Inbound: the provider is converting the payment to the user's balance
    /// asset.
    Converting,
    /// Inbound: the provider started a top-up with `payment.topUp` to credit
    /// the user. A partial claim may be followed by further top-ups.
    Crediting {
        /// Id the top-up was started with.
        top_up_id: [u8; 32],
        /// Amount the top-up asks to credit.
        amount: u128,
    },
    /// Inbound: the provider started every top-up it will. The session is
    /// delivered with what they claimed.
    Delivered,
    /// Outbound: the provider asked the user to pay it with
    /// `payment.request`. The session is released once that payment
    /// completes.
    Collecting {
        /// Id the payment request was made with.
        payment_id: [u8; 32],
        /// Amount the payment asks for.
        amount: u128,
    },
    /// The session ended without success, before any funds moved.
    Failed {
        /// Why it ended.
        reason: FundingFailure,
    },
}

/// Item of [`crate::api::FundingProvider::serve_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingServeSubscribeItem {
    /// A session the user chose this provider for, or one still in flight
    /// when the stream opened.
    Assigned {
        /// The session.
        session: FundingAssignment,
    },
    /// The user asked to cancel a session. The provider reports `Failed`
    /// with `Cancelled` if it can still stop.
    Cancel {
        /// Session to cancel.
        intent: String,
    },
}

/// Error from [`crate::api::FundingProvider::serve_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingServeSubscribeError {
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to report progress on a session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingReportRequest {
    /// Session the update is about.
    pub intent: String,
    /// The update.
    pub update: FundingUpdate,
}

/// Error from [`crate::api::FundingProvider::report`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingReportError {
    /// No open session with this id is assigned to the caller.
    NotFound,
    /// The update does not follow the session's last one, or does not fit
    /// its direction.
    OutOfOrder,
    /// The top-up or payment request it names is already named by another of
    /// the caller's sessions.
    DuplicateId,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to show one of the provider's screens for a session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingPresentFrameRequest {
    /// Session the screen belongs to.
    pub intent: String,
    /// Route of the provider's App to show, such as its KYC or card entry.
    pub route: String,
}

/// How a provider screen closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingFrameOutcome {
    /// The provider's screen finished and closed itself.
    Closed,
    /// The user closed it.
    Dismissed,
}

/// Response of [`crate::api::FundingProvider::present_frame`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingPresentFrameResponse {
    /// How the screen closed.
    pub outcome: FundingFrameOutcome,
}

/// Error from [`crate::api::FundingProvider::present_frame`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingPresentFrameError {
    /// No open session with this id is assigned to the caller.
    NotFound,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
