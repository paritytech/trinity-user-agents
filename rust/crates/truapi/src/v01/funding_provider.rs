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
    /// The quote the user chose this provider on, when it was quoted.
    pub quote: Option<FundingQuote>,
}

/// How the user pays or is paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingRail {
    /// A card payment.
    Card,
    /// A bank transfer.
    Bank,
    /// A crypto transfer.
    Crypto,
}

/// What the host asks a provider to price.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingQuoteAsk {
    /// Which way value moves.
    pub direction: FundingDirection,
    /// How the user pays or is paid.
    pub rail: FundingRail,
    /// Symbol the user pays with (In) or receives (Out), as the provider's
    /// manifest names it.
    pub asset: String,
    /// Amount in the user's payment balance units: credited for In, debited
    /// for Out.
    pub amount: u128,
    /// ISO 3166-1 alpha-2 code of the user's country, when the host knows it.
    pub country: Option<String>,
}

/// A provider's price for an ask.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingQuote {
    /// The provider's id for this quote, handed back when the user picks it.
    pub quote_id: String,
    /// What the user pays, in the smallest unit of the ask's asset for In and
    /// in balance units for Out.
    pub send_amount: u128,
    /// What the user receives, in balance units for In and in the smallest
    /// unit of the ask's asset for Out.
    pub receive_amount: u128,
    /// The provider's fee, in the unit of `send_amount`.
    pub provider_fee: u128,
    /// Network fees, in the unit of `send_amount`.
    pub network_fee: u128,
    /// Expected seconds until the funds arrive, when the provider says.
    pub eta_secs: Option<u64>,
    /// When the provider stops honouring the quote, in Unix milliseconds.
    pub expires_at: Option<u64>,
}

/// Why a provider will not price an ask.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingQuoteRefusal {
    /// Not offered in the user's country.
    CountryUnsupported,
    /// Below the provider's minimum, in the ask's balance units.
    BelowMinimum {
        /// The smallest amount the provider takes.
        min: u128,
    },
    /// Above the provider's maximum, in the ask's balance units.
    AboveMaximum {
        /// The largest amount the provider takes.
        max: u128,
    },
    /// The provider cannot serve the ask right now.
    Unavailable,
    /// Outcome not covered above.
    Other {
        /// Human-readable reason.
        message: String,
    },
}

/// A provider's answer to an ask.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FundingQuoteAnswer {
    /// The provider's price.
    Quoted {
        /// The quote.
        quote: FundingQuote,
    },
    /// The provider will not price it.
    Refused {
        /// Why.
        reason: FundingQuoteRefusal,
    },
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
    /// The host asks for a price, answered with `answerQuote` and this
    /// `ask_id`. An ask still unanswered when the stream opens is sent again.
    Quote {
        /// Id the answer names.
        ask_id: String,
        /// What to price.
        ask: FundingQuoteAsk,
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

/// Request to answer a quote ask.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingAnswerQuoteRequest {
    /// The ask being answered.
    pub ask_id: String,
    /// The answer.
    pub answer: FundingQuoteAnswer,
}

/// Error from [`crate::api::FundingProvider::answer_quote`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingAnswerQuoteError {
    /// No open ask with this id was sent to the caller; it may have timed out.
    NotFound,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
