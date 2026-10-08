use parity_scale_codec::{Decode, Encode};

/// Which way value crosses the boundary between the user's Polkadot balance
/// and everything outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingDirection {
    /// Value moves in, credited to the user's balance.
    In,
    /// Value moves out of the user's balance under the user's authorization;
    /// the off-chain leg is the provider's obligation.
    Out,
}

/// Why a session ended without success.
///
/// Carries an outcome, never a rule, so a client renders these without holding
/// any part of the operator's compliance logic.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingFailure {
    /// Not offered in the user's market.
    RegionUnavailable,
    /// The user must complete verification first.
    VerificationRequired,
    /// Verification was attempted and refused.
    VerificationRefused,
    /// Below the route's minimum.
    BelowMinimum,
    /// Above the route's maximum.
    AboveMaximum,
    /// The user's balance does not cover an outbound session.
    InsufficientBalance,
    /// The session's window closed before it completed.
    Expired,
    /// Funds arrived in the wrong asset or on the wrong chain.
    WrongAssetOrChain,
    /// The provider stopped responding.
    ProviderTimeout,
    /// The user abandoned the flow, or declined to authorize an outbound
    /// release.
    Cancelled,
    /// Outcome not covered above. Clients render the message and treat the
    /// code as opaque.
    Other {
        /// Stable machine-readable code.
        code: String,
        /// Human-readable text, already localized by the host.
        message: String,
    },
}

/// Request to open the funding modality.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingRequest {
    /// Which way value moves; the host opens on the matching screen.
    pub direction: FundingDirection,
    /// Amount sought, in the user's payment balance units. `None` lets the
    /// user choose.
    pub amount: Option<u128>,
}

/// Accepted intent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingResponse {
    /// Session id, durable across host restarts, used to watch the session.
    pub intent: String,
}

/// Error from [`crate::api::Funding::request`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingError {
    /// User is not logged in.
    NotConnected,
    /// The user dismissed the funding screen without starting a session.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to watch a funding session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFundingStatusSubscribeRequest {
    /// Session to watch.
    pub intent: String,
}

/// Progress of a funding session, ending in exactly one terminal item.
///
/// `Delivered` means the funds reached the user's balance. `Released` means
/// they left it under the user's authorization, and says nothing about the
/// off-chain leg: no host can verify that cash reached a bank.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum HostFundingStatusSubscribeItem {
    /// In flight. The host draws its steps; a product only waits for the end.
    InProgress {
        /// When the session expires unless funds are already moving, in Unix
        /// milliseconds. `None` once it no longer expires.
        expires_at: Option<u64>,
    },
    /// Inbound terminal success. Funds credited to the user's balance.
    Delivered {
        /// Amount credited, which may differ from the amount requested.
        credited: u128,
    },
    /// Outbound terminal success. Funds left the user's balance under the
    /// user's authorization.
    Released {
        /// Amount debited.
        debited: u128,
    },
    /// Terminal failure.
    Failed {
        /// Why it ended.
        reason: FundingFailure,
        /// Amount moved before the failure. May be non-zero.
        moved: u128,
    },
}

/// Error from [`crate::api::Funding::status_subscribe`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFundingStatusSubscribeError {
    /// No such session, or it does not belong to the caller.
    NotFound,
    /// Catch-all.
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
            (
                FundingDirection::In.encode(),
                FundingDirection::Out.encode()
            ),
            (vec![0], vec![1])
        );
    }
}
