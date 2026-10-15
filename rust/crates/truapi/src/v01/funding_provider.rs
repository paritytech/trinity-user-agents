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
	/// When the session ends as `Expired` if nothing changes, in Unix
	/// milliseconds: 7 days after the provider's last update while no funds
	/// move. `None` once a top-up or payment request is moving funds, as only
	/// its outcome ends the session then.
	pub expires_at: Option<u64>,
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
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
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
	/// The provider will not cancel a session the user asked to cancel once
	/// money moved. The session carries on. Outside the order of steps.
	CancelRefused {
		/// Why, shown to the user.
		reason: String,
	},
	/// The provider can cancel a session the user asked to cancel once money
	/// moved, on these terms. The user accepts or declines in the host, and
	/// the provider hears which on its serve stream. Outside the order of
	/// steps.
	CancelOffer {
		/// What the provider keeps, in the user's payment balance units.
		fee: u128,
		/// What the user gets back, in the user's payment balance units.
		refund: u128,
		/// When the offer lapses, in Unix milliseconds. A lapsed offer is
		/// declined.
		expires_at: Option<u64>,
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
	/// The user asked to cancel a session. Before money moved the provider
	/// reports `Failed` with `Cancelled` if it can still stop. After, it
	/// answers with `CancelOffer` or `CancelRefused`.
	Cancel {
		/// Session to cancel.
		intent: String,
	},
	/// The user accepted the provider's `CancelOffer`. The provider settles
	/// the cancel: it refunds the user and ends the session.
	CancelAccepted {
		/// The session.
		intent: String,
	},
	/// The user declined the provider's `CancelOffer`, or it lapsed. The
	/// session carries on.
	CancelDeclined {
		/// The session.
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
	/// The top-up or payment request it names was not started by the
	/// caller with `payment.topUp` or `payment.request` for this session since
	/// it was assigned, asks a different amount, or was already named.
	NotStarted,
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
	/// What the frame shows.
	pub target: FundingFrameTarget,
}

/// What a provider frame shows.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingFrameTarget {
	/// A route of the provider's own App, such as its KYC or card entry.
	App {
		/// The route.
		route: String,
	},
	/// An external https page, such as an onramp's widget, shown with its
	/// origin visible.
	Page {
		/// The page to open.
		url: String,
		/// An https URL the page navigates to when it is done. The host
		/// closes the frame on reaching it and answers `Returned`.
		return_url: Option<String>,
	},
}

/// How a provider screen closed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingFrameOutcome {
	/// The provider's screen finished and closed itself.
	Closed,
	/// The user closed it.
	Dismissed,
	/// The page navigated to the request's `return_url`.
	Returned {
		/// The full URL it navigated to, query included.
		url: String,
	},
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
	/// An App route that leaves the provider's App, or a page or return URL
	/// that is not an https URL with a host, or that carries credentials.
	InvalidUrl,
	/// Catch-all.
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
}
