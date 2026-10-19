use parity_scale_codec::{Decode, Encode};

/// Which way value crosses the boundary between the user's Polkadot balance
/// and everything outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingDirection {
	/// Value moves in, credited to the user's balance.
	#[codec(index = 0)]
	In,
	/// Value moves out of the user's balance under the user's authorization;
	/// the off-chain leg is the provider's obligation.
	#[codec(index = 1)]
	Out,
}

/// Why a session ended without success.
///
/// Carries an outcome, never a rule, so a client renders these without holding
/// any part of the operator's compliance logic.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingFailure {
	/// Not offered in the user's market.
	#[codec(index = 0)]
	RegionUnavailable,
	/// The user must complete verification first.
	#[codec(index = 1)]
	VerificationRequired,
	/// Verification was attempted and refused.
	#[codec(index = 2)]
	VerificationRefused,
	/// Below the route's minimum.
	#[codec(index = 3)]
	BelowMinimum,
	/// Above the route's maximum.
	#[codec(index = 4)]
	AboveMaximum,
	/// The user's balance does not cover an outbound session.
	#[codec(index = 5)]
	InsufficientBalance,
	/// The session's window closed before it completed.
	#[codec(index = 6)]
	Expired,
	/// Funds arrived in the wrong asset or on the wrong chain.
	#[codec(index = 7)]
	WrongAssetOrChain,
	/// The provider stopped responding.
	#[codec(index = 8)]
	ProviderTimeout,
	/// The user abandoned the flow, or declined to authorize an outbound
	/// release.
	#[codec(index = 9)]
	Cancelled,
	/// The provider returned the user's payment instead of crediting it.
	#[codec(index = 11)]
	Refunded,
	/// The user's bank or card issuer refused the payment.
	#[codec(index = 12)]
	Declined,
	/// Outcome not covered by the other variants. A client shows `message`,
	/// except for the codes the core sets itself, which it may show in its
	/// own words.
	#[codec(index = 10)]
	Other {
		/// Stable machine-readable code. The core sets `not_claimed` when
		/// nothing reached the balance and `payment_failed` when the user's
		/// payment did not go through; any other code is the provider's own.
		code: String,
		/// Text to show: the provider's own, or English text for the codes
		/// the core sets.
		message: String,
	},
}

/// Request to open the funding modality.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingRequest {
	/// Caller-chosen id the session is watched by, so a product that reloads
	/// while the overlay is up can still watch it. Reusing one is refused
	/// with `AlreadyExists`.
	pub id: [u8; 32],
	/// Which way value moves; the host opens on the matching screen.
	pub direction: FundingDirection,
	/// Amount sought, in the user's payment balance units. `None` lets the
	/// user choose.
	pub amount: Option<u128>,
}

/// Error from [`crate::api::Funding::request`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingError {
	/// User is not logged in.
	#[codec(index = 0)]
	NotConnected,
	/// The user dismissed the funding screen without starting a session.
	#[codec(index = 1)]
	Rejected,
	/// The amount sought is zero.
	#[codec(index = 2)]
	InvalidAmount,
	/// Catch-all.
	#[codec(index = 3)]
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
	/// This product already has a session with this id.
	#[codec(index = 4)]
	AlreadyExists,
}

/// Request to watch a funding session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingStatusSubscribeRequest {
	/// The id the session was requested with.
	pub id: [u8; 32],
}

/// Progress of a funding session, ending in exactly one terminal item.
///
/// `Delivered` means the funds reached the user's balance. `Released` means
/// they left it under the user's authorization, and says nothing about the
/// off-chain leg: no host can verify that cash reached a bank.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum HostFundingStatusSubscribeItem {
	/// In flight. The host draws its steps; a product only waits for the end.
	#[codec(index = 0)]
	InProgress {
		/// When the session expires unless funds are already moving, in Unix
		/// milliseconds. `None` once it no longer expires.
		expires_at: Option<u64>,
	},
	/// Inbound terminal success. Funds credited to the user's balance.
	#[codec(index = 1)]
	Delivered {
		/// Amount credited, which may differ from the amount requested.
		credited: u128,
	},
	/// Outbound terminal success. Funds left the user's balance under the
	/// user's authorization.
	#[codec(index = 2)]
	Released {
		/// Amount debited.
		debited: u128,
	},
	/// Terminal failure.
	#[codec(index = 3)]
	Failed {
		/// Why it ended.
		reason: FundingFailure,
	},
}

/// Error from [`crate::api::Funding::status_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingStatusSubscribeError {
	/// The caller has no session with this id. An ended session stays
	/// watchable for good, compacted once newer ones push it out, and
	/// watching it yields its terminal item.
	#[codec(index = 0)]
	NotFound,
	/// Catch-all.
	#[codec(index = 1)]
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
}

#[cfg(test)]
mod tests {
	use super::*;

	// Persisted sessions and the wire both carry the direction, so its order
	// is fixed.
	#[test]
	fn direction_discriminants_are_stable() {
		assert_eq!(
			(FundingDirection::In.encode(), FundingDirection::Out.encode()),
			(vec![0], vec![1])
		);
	}
}
