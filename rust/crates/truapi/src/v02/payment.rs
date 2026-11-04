//! Payment messages v0.2 redefines: a top-up or payment a funding provider's
//! worker starts names the funding session it is for, and is refused with
//! `FundingNotServed` when that session does not take it.

use parity_scale_codec::{Decode, Encode};

use crate::v01::PaymentTopUpSource;

/// Request to top up the product payment balance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
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
	/// The funding session this top-up credits, when a funding provider's
	/// worker starts it for one it serves. Only such a top-up can be named in
	/// that session's reports. A repeat under the same id must ask the same.
	#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), uniffi(default))]
	pub funding: Option<String>,
}

/// Request to initiate a payment to another account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostPaymentRequest {
	/// Optional purse selector. `None` means MAIN_PURSE.
	pub from: Option<u32>,
	/// Amount to pay.
	pub amount: u128,
	/// Destination account.
	pub destination: [u8; 32],
	/// Caller-chosen id the payment's status is followed by. Reusing one is
	/// refused with `AlreadyExists`, which makes a retried call safe.
	pub id: [u8; 32],
	/// The funding session this payment collects for, when a funding
	/// provider's worker requests it for one it serves. Only such a payment
	/// can be named in that session's reports. A repeat under the same id must
	/// ask the same.
	#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), uniffi(default))]
	pub funding: Option<String>,
}

/// Error from [`crate::api::Payment::top_up`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Error))]
pub enum HostPaymentTopUpError {
	/// The source key is malformed, or the source was not found.
	#[display("invalid top-up source")]
	InvalidSource,
	/// Another top-up from the same source is still running.
	#[display("another top-up from this source is running")]
	SourceBusy,
	/// The amount is zero.
	#[display("the top-up amount is zero")]
	InvalidAmount,
	/// Catch-all.
	#[display("{reason}")]
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
	/// A top-up with this id already exists.
	#[display("a top-up with this id already exists")]
	AlreadyExists,
	/// The funding session named is not open and served by this worker for
	/// inbound funds, or holds as many top-ups as it may.
	#[display("the funding session does not take this top-up")]
	FundingNotServed,
}

/// Error from [`crate::api::Payment::request`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::Display)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Error))]
pub enum HostPaymentError {
	/// User rejected the payment request.
	#[display("the payment was declined")]
	Rejected,
	/// User's available balance is not sufficient for the requested amount.
	#[display("the balance does not cover the payment")]
	InsufficientBalance,
	/// Catch-all.
	#[display("{reason}")]
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
	/// The amount is zero.
	#[display("the payment amount is zero")]
	InvalidAmount,
	/// A payment with this id already exists.
	#[display("a payment with this id already exists")]
	AlreadyExists,
	/// The funding session named is not open and served by this worker for
	/// outbound funds, or holds as many payments as it may.
	#[display("the funding session does not take this payment")]
	FundingNotServed,
}
