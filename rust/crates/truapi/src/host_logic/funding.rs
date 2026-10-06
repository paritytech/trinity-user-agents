//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death through
//! [`CoreStorageKey::FundingSessions`], and is re-attached by
//! `Funding::status_subscribe`. A session always terminates, because the core
//! expires it on its own clock.

use parity_scale_codec::{Decode, Encode};
use tracing::warn;
use truapi::latest::{FundingDirection, FundingFailure, HostFundingStatusSubscribeItem};

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has recorded the core keeps, most
/// recently ended first.
const SETTLED_HISTORY_LIMIT: usize = 50;
/// How far back, counting every ended session newest first, those the host
/// has not recorded yet are kept.
const UNACKNOWLEDGED_LIMIT: usize = 200;

/// What the core knows about one session, independent of any host surface.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingSession {
    /// Identifier handed back to the caller and used to re-attach.
    pub intent: String,
    /// Product that opened the session and the only one that may watch it.
    /// `None` when the host opened it, as the Balance card does.
    pub owner_product_id: Option<String>,
    /// Which way value crosses the boundary.
    pub direction: FundingDirection,
    /// Amount sought, in the user's payment balance units. `None` until the
    /// user chooses one.
    pub amount: Option<u128>,
    /// Current stage.
    pub stage: FundingStage,
    /// When the session was opened, in Unix milliseconds.
    pub opened_at_ms: u64,
    /// When the session expires if still open, in Unix milliseconds.
    pub deadline_ms: u64,
    /// Whether the host has recorded the session's outcome in its own
    /// history. An ended session is handed to the host until it has.
    pub acknowledged: bool,
}

/// Stage of a session, as the core persists it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingStage {
    /// In flight.
    Open,
    /// Ended without success.
    Failed {
        /// Why it ended.
        reason: FundingFailure,
        /// When it ended, in Unix milliseconds.
        settled_at_ms: u64,
    },
}

impl FundingSession {
    /// Open a session that expires one window from `now_ms`.
    pub fn new(
        intent: String,
        owner_product_id: Option<String>,
        direction: FundingDirection,
        amount: Option<u128>,
        now_ms: u64,
    ) -> Self {
        Self {
            intent,
            owner_product_id,
            direction,
            amount,
            stage: FundingStage::Open,
            opened_at_ms: now_ms,
            deadline_ms: now_ms.saturating_add(SESSION_WINDOW_MS),
            acknowledged: false,
        }
    }

    /// Whether the host still has to hear of the session when funding
    /// resumes: it is in flight, or it ended and the host has not recorded
    /// the outcome.
    pub fn needs_handoff(&self) -> bool {
        !self.is_terminal() || !self.acknowledged
    }

    /// End an open session as cancelled. Returns whether it changed.
    pub fn cancel(&mut self, now_ms: u64) -> bool {
        self.fail(FundingFailure::Cancelled, now_ms)
    }

    /// Whether the session has ended.
    pub fn is_terminal(&self) -> bool {
        self.settled_at_ms().is_some()
    }

    /// When the session ended, if it has.
    pub fn settled_at_ms(&self) -> Option<u64> {
        match self.stage {
            FundingStage::Open => None,
            FundingStage::Failed { settled_at_ms, .. } => Some(settled_at_ms),
        }
    }

    /// Project the current stage onto the wire item subscribers receive.
    pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
        match (&self.stage, self.direction) {
            (FundingStage::Open, FundingDirection::In) => {
                HostFundingStatusSubscribeItem::AwaitingDeposit {
                    expires_at: Some(self.deadline_ms),
                }
            }
            (FundingStage::Open, FundingDirection::Out) => {
                HostFundingStatusSubscribeItem::AwaitingRelease
            }
            (FundingStage::Failed { reason, .. }, _) => HostFundingStatusSubscribeItem::Failed {
                reason: reason.clone(),
                moved: 0,
            },
        }
    }

    /// End the session with `reason`, unless it already ended. Returns whether
    /// it changed.
    pub fn fail(&mut self, reason: FundingFailure, now_ms: u64) -> bool {
        if self.is_terminal() {
            return false;
        }
        self.stage = FundingStage::Failed {
            reason,
            settled_at_ms: now_ms,
        };
        true
    }

    /// Expire the session if it is still open at its deadline. Returns whether
    /// it expired.
    pub fn expire_if_due(&mut self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms && self.fail(FundingFailure::Expired, now_ms)
    }
}

/// Why a session operation failed.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum FundingSessionError {
    /// The host's storage callback failed.
    #[display("funding session storage failed: {reason}")]
    Storage {
        /// Failure detail.
        reason: String,
    },
}

/// The sessions worth keeping: every open one, then the most recently ended
/// ones up to fixed bounds, longer for those the host has not recorded.
///
/// The bound exists because [`CoreStorageKey::FundingSessions`] is one SCALE
/// blob rewritten on every change; the host keeps the full history.
pub fn retained(sessions: impl IntoIterator<Item = FundingSession>) -> Vec<FundingSession> {
    let (mut open, mut settled): (Vec<_>, Vec<_>) = sessions
        .into_iter()
        .partition(|session| !session.is_terminal());
    settled.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
    // One the host has not recorded yet is kept further back, so a host that
    // was away still receives it, but not without bound.
    let mut recorded = 0;
    let mut kept: Vec<_> = settled
        .into_iter()
        .enumerate()
        .filter(|(newest, session)| {
            if !session.acknowledged {
                return *newest < UNACKNOWLEDGED_LIMIT;
            }
            recorded += 1;
            recorded <= SETTLED_HISTORY_LIMIT
        })
        .map(|(_, session)| session)
        .collect();
    open.append(&mut kept);
    open
}

/// Read every persisted session.
///
/// A blob that does not decode is discarded with a warning rather than
/// returned as an error, so one bad write cannot disable funding for good.
pub async fn load_sessions(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
    let Some(blob) = storage
        .read_core_storage(CoreStorageKey::FundingSessions)
        .await
        .map_err(|err| FundingSessionError::Storage { reason: err.reason })?
    else {
        return Ok(Vec::new());
    };
    match Vec::<FundingSession>::decode(&mut blob.as_slice()) {
        Ok(sessions) if sessions.encoded_size() == blob.len() => Ok(sessions),
        _ => {
            warn!("discarding undecodable funding sessions");
            Ok(Vec::new())
        }
    }
}

/// Replace the persisted set with `sessions`, clearing the slot when empty.
pub async fn store_sessions(
    storage: &(impl CoreStorage + ?Sized),
    sessions: &[FundingSession],
) -> Result<(), FundingSessionError> {
    let written = if sessions.is_empty() {
        storage
            .clear_core_storage(CoreStorageKey::FundingSessions)
            .await
    } else {
        storage
            .write_core_storage(CoreStorageKey::FundingSessions, sessions.encode())
            .await
    };
    written.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::executor::block_on;

    use crate::test_support::stub_platform;

    const NOW: u64 = 1_700_000_000_000;

    fn session(direction: FundingDirection) -> FundingSession {
        FundingSession::new(
            "fs_1".to_string(),
            Some("wallet.dot".to_string()),
            direction,
            Some(100),
            NOW,
        )
    }

    fn expired(intent: &str, settled_at_ms: u64) -> FundingSession {
        FundingSession {
            intent: intent.to_string(),
            stage: FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms,
            },
            ..session(FundingDirection::In)
        }
    }

    #[test]
    fn an_open_session_shows_the_screen_for_its_direction() {
        assert_eq!(
            (
                session(FundingDirection::In).wire_item(),
                session(FundingDirection::Out).wire_item(),
            ),
            (
                HostFundingStatusSubscribeItem::AwaitingDeposit {
                    expires_at: Some(NOW + SESSION_WINDOW_MS),
                },
                HostFundingStatusSubscribeItem::AwaitingRelease,
            )
        );
    }

    // Nothing else ends a session that hears nothing, so expiry is what
    // guarantees every subscriber eventually gets a terminal item.
    #[test]
    fn a_session_expires_at_its_deadline_and_not_before() {
        let mut session = session(FundingDirection::In);

        assert!(!session.expire_if_due(NOW + SESSION_WINDOW_MS - 1));
        assert!(session.expire_if_due(NOW + SESSION_WINDOW_MS));
        assert_eq!(
            session.wire_item(),
            HostFundingStatusSubscribeItem::Failed {
                reason: FundingFailure::Expired,
                moved: 0,
            }
        );
    }

    #[test]
    fn an_ended_session_keeps_its_first_outcome() {
        let mut session = expired("fs_1", NOW);

        assert!(!session.fail(FundingFailure::Cancelled, NOW + 1));
        assert_eq!(session, expired("fs_1", NOW));
    }

    #[test]
    fn open_sessions_come_first_and_recorded_history_keeps_the_newest() {
        let open = session(FundingDirection::Out);
        let settled = (0..SETTLED_HISTORY_LIMIT + 1).map(|index| FundingSession {
            acknowledged: true,
            ..expired(&format!("fs_s{index}"), NOW + index as u64)
        });

        let kept: Vec<String> = retained(settled.chain([open]))
            .into_iter()
            .map(|session| session.intent)
            .collect();

        let newest_first = (1..=SETTLED_HISTORY_LIMIT).rev().map(|index| format!("fs_s{index}"));
        assert_eq!(
            kept,
            std::iter::once("fs_1".to_string())
                .chain(newest_first)
                .collect::<Vec<_>>()
        );
    }

    // A host that was away must still receive every outcome for its own
    // history, so one it has not recorded outlives the recorded ones, though
    // not without bound.
    #[test]
    fn history_keeps_what_the_host_has_not_recorded() {
        let ended = |index: usize, acknowledged| FundingSession {
            acknowledged,
            ..expired(&format!("fs_s{index}"), NOW + index as u64)
        };
        let oldest_unrecorded = ended(0, false);
        let recorded = (1..=SETTLED_HISTORY_LIMIT + 1).map(|index| ended(index, true));
        let unrecorded = (100..100 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

        let kept = retained(recorded.chain(unrecorded).chain([oldest_unrecorded.clone()]));

        assert_eq!(
            (
                kept.iter().filter(|session| session.acknowledged).count(),
                kept.iter().filter(|session| !session.acknowledged).count(),
                kept.contains(&oldest_unrecorded),
            ),
            (SETTLED_HISTORY_LIMIT, UNACKNOWLEDGED_LIMIT, false)
        );
    }

    // Cancel ends an open session once; an ended one keeps its outcome.
    #[test]
    fn a_session_is_cancelled_once() {
        let mut open = session(FundingDirection::In);
        let mut ended = expired("fs_2", NOW);

        assert_eq!(
            (open.cancel(NOW + 1), open.wire_item(), ended.cancel(NOW + 1)),
            (
                true,
                HostFundingStatusSubscribeItem::Failed {
                    reason: FundingFailure::Cancelled,
                    moved: 0,
                },
                false
            )
        );
    }

    #[test]
    fn sessions_round_trip_through_core_storage() {
        let storage = stub_platform();
        let sessions = vec![session(FundingDirection::In), expired("fs_2", NOW)];

        block_on(store_sessions(storage.as_ref(), &sessions)).expect("stored");

        assert_eq!(
            block_on(load_sessions(storage.as_ref())).expect("loaded"),
            sessions
        );
    }

    #[test]
    fn an_undecodable_blob_reads_as_no_sessions() {
        let storage = stub_platform();
        let mut blob = vec![session(FundingDirection::In)].encode();
        blob.push(0xff);
        block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob))
            .expect("written");

        assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
    }

    #[test]
    fn storing_nothing_clears_the_slot() {
        let storage = stub_platform();
        block_on(store_sessions(
            storage.as_ref(),
            &[session(FundingDirection::In)],
        ))
        .expect("stored");

        block_on(store_sessions(storage.as_ref(), &[])).expect("cleared");

        assert_eq!(
            block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)),
            Ok(None)
        );
    }
}
