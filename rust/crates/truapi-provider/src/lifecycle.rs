//! What the embedded light client is doing on one chain, as reported by
//! smoldot's lifecycle service and watched through
//! [`EmbeddedChainProvider::lifecycle`](crate::EmbeddedChainProvider::lifecycle).
//!
//! These mirror smoldot's `lifecycle_service` types, whose schema smoldot marks
//! unstable, so a change there stays behind this crate's own types.

use smoldot_light::lifecycle_service;

/// Lifecycle state of a chain. Every update carries the whole state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "uniffi", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ChainLifecycle {
    /// How far the chain has bootstrapped.
    pub phase: ChainPhase,
    /// Number of peers currently connected on this chain.
    pub peers: u32,
    /// Whether the chain is making progress.
    pub health: ChainHealth,
}

/// Bootstrap progress of a chain.
///
/// A parachain goes from `Connecting` straight to `Ready`: its progress is the
/// progress of its relay, which is the chain to watch for a progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "uniffi", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ChainPhase {
    /// The chain has been added and no block is being streamed yet.
    Connecting,
    /// A warp sync is in progress.
    Syncing {
        /// Highest block proven finalized so far.
        at: u64,
        /// Highest best block a connected peer advertises. Never below `at`.
        target: u64,
    },
    /// The chain is synced and following new blocks. Not terminal: a later
    /// warp sync moves the chain back to `Syncing`.
    Ready,
}

/// Verdict of smoldot's stall watchdog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "uniffi", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ChainHealth {
    /// The chain is progressing, or has not been stuck long enough to say.
    Ok,
    /// The chain has stopped progressing.
    Stalled {
        /// Why the chain is considered stalled.
        reason: StallReason,
    },
}

/// Why a chain is considered stalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "uniffi", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum StallReason {
    /// No peer has been connected for 30 seconds.
    NoPeers,
    /// A warp sync has not advanced for 45 seconds.
    NoProgress,
}

impl From<lifecycle_service::LifecycleState> for ChainLifecycle {
    fn from(state: lifecycle_service::LifecycleState) -> Self {
        ChainLifecycle {
            phase: match state.phase {
                lifecycle_service::Phase::Connecting => ChainPhase::Connecting,
                lifecycle_service::Phase::Syncing { at, target } => {
                    ChainPhase::Syncing { at, target }
                }
                lifecycle_service::Phase::Ready => ChainPhase::Ready,
            },
            peers: state.num_peers,
            health: match state.health {
                lifecycle_service::Health::Ok => ChainHealth::Ok,
                lifecycle_service::Health::Stalled { reason } => {
                    ChainHealth::Stalled {
                        reason: match reason {
                            lifecycle_service::StallReason::NoPeers => StallReason::NoPeers,
                            lifecycle_service::StallReason::NoProgress => StallReason::NoProgress,
                        },
                    }
                }
            },
        }
    }
}
