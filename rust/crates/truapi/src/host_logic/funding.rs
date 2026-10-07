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
use truapi::latest::{
    FundingAssignment, FundingDirection, FundingFailure, FundingQuote, FundingUpdate,
    HostFundingStatusSubscribeItem,
};

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has recorded the core keeps, most
/// recently ended first.
const SETTLED_HISTORY_LIMIT: usize = 50;
/// How far back, counting every ended session newest first, those the host
/// has not recorded yet are kept.
const UNACKNOWLEDGED_LIMIT: usize = 200;
/// Most top-ups a provider may start for one session, as getcash claims in
/// up to three attempts.
const TOP_UP_LIMIT: usize = 3;

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
    /// Product id of the provider the user chose. `None` until they choose.
    pub provider_id: Option<String>,
    /// Whether the user asked to cancel and the provider has not answered.
    pub cancel_requested: bool,
    /// Every update the provider reported, oldest first.
    pub updates: Vec<FundingUpdateRecord>,
    /// The quote the user chose the provider on, when it was quoted.
    pub quote: Option<FundingQuote>,
}

/// One update a provider reported, and when the core stored it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingUpdateRecord {
    /// The update.
    pub update: FundingUpdate,
    /// When it was stored, in Unix milliseconds.
    pub at_ms: u64,
}

/// Why a provider's report was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportRefusal {
    /// The session is not open or not assigned to the reporting provider.
    NotFound,
    /// The update does not follow the last one, or does not fit the
    /// session's direction.
    OutOfOrder,
    /// The top-up or payment request it names is already named by another
    /// session of the same provider.
    DuplicateId,
}

/// What a cancel did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// No provider serves the session, so it ended as cancelled.
    Ended,
    /// The provider serving it was asked to stop.
    Requested,
    /// Too late: the session ended, or the user's payment already reached
    /// the provider.
    Refused,
}

/// What the core follows to decide a session's outcome once funds move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settlement {
    /// The top-ups an inbound session credits through, with the amount each
    /// asks for.
    TopUps(Vec<([u8; 32], u128)>),
    /// The payment an outbound session collects through, with its amount.
    Payment([u8; 32], u128),
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
    /// Inbound success.
    Delivered {
        /// Amount credited, which may differ from the amount requested.
        credited: u128,
        /// When it ended, in Unix milliseconds.
        settled_at_ms: u64,
    },
    /// Outbound success.
    Released {
        /// Amount debited.
        debited: u128,
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
            provider_id: None,
            cancel_requested: false,
            updates: Vec::new(),
            quote: None,
        }
    }

    /// Assign the session to the provider the user chose, on the quote they
    /// chose it by. Returns whether it was open and not yet assigned.
    pub fn assign(&mut self, provider_id: &str, quote: Option<FundingQuote>) -> bool {
        if self.is_terminal() || self.provider_id.is_some() {
            return false;
        }
        self.provider_id = Some(provider_id.to_string());
        self.quote = quote;
        true
    }

    /// The session as the provider serving it receives it.
    pub fn assignment(&self) -> FundingAssignment {
        FundingAssignment {
            intent: self.intent.clone(),
            direction: self.direction,
            amount: self.amount,
            expires_at: self.deadline_ms,
            last_update: self.last_update().cloned(),
            quote: self.quote.clone(),
        }
    }

    /// The provider's most recent update.
    pub fn last_update(&self) -> Option<&FundingUpdate> {
        self.updates.last().map(|record| &record.update)
    }

    /// Store `update` from `provider_id`. A `Failed` update ends the session.
    pub fn report(
        &mut self,
        provider_id: &str,
        update: FundingUpdate,
        now_ms: u64,
    ) -> Result<(), ReportRefusal> {
        if self.is_terminal() || self.provider_id.as_deref() != Some(provider_id) {
            return Err(ReportRefusal::NotFound);
        }
        if !self.follows(&update) {
            return Err(ReportRefusal::OutOfOrder);
        }
        if let FundingUpdate::Failed { reason } = &update {
            self.stage = FundingStage::Failed {
                reason: reason.clone(),
                settled_at_ms: now_ms,
            };
        }
        self.updates.push(FundingUpdateRecord {
            update,
            at_ms: now_ms,
        });
        Ok(())
    }

    /// Whether `update` may come next. Updates only move forward, a session
    /// may credit through a few top-ups, and once funds move the provider
    /// can no longer fail it: the top-ups or payment decide.
    fn follows(&self, update: &FundingUpdate) -> bool {
        let fits_direction = match update {
            FundingUpdate::Collecting { .. } => self.direction == FundingDirection::Out,
            FundingUpdate::Failed { .. } => true,
            _ => self.direction == FundingDirection::In,
        };
        let last = self.last_update().map(update_rank);
        let top_ups = self.top_ups();
        fits_direction
            && match update {
                FundingUpdate::Failed { .. } => !self.funds_moving(),
                FundingUpdate::Crediting { top_up_id, .. } => {
                    last <= Some(update_rank(update))
                        && top_ups.len() < TOP_UP_LIMIT
                        && top_ups.iter().all(|(id, _)| id != top_up_id)
                }
                FundingUpdate::Delivered => last < Some(update_rank(update)) && !top_ups.is_empty(),
                _ => last < Some(update_rank(update)),
            }
    }

    /// The top-up or payment request id `update` names, if any.
    pub fn named_id(update: &FundingUpdate) -> Option<[u8; 32]> {
        match update {
            FundingUpdate::Crediting { top_up_id, .. } => Some(*top_up_id),
            FundingUpdate::Collecting { payment_id, .. } => Some(*payment_id),
            _ => None,
        }
    }

    /// Whether any of this session's updates names `id`.
    pub fn names(&self, id: &[u8; 32]) -> bool {
        self.updates
            .iter()
            .any(|record| Self::named_id(&record.update).as_ref() == Some(id))
    }

    /// The top-ups the provider started, in order, with the amount each asks
    /// for.
    fn top_ups(&self) -> Vec<([u8; 32], u128)> {
        self.updates
            .iter()
            .filter_map(|record| match record.update {
                FundingUpdate::Crediting { top_up_id, amount } => Some((top_up_id, amount)),
                _ => None,
            })
            .collect()
    }

    /// Whether funds are moving: the provider started a top-up or a payment
    /// request. From then on the session neither expires nor cancels.
    pub fn funds_moving(&self) -> bool {
        self.updates.iter().any(|record| {
            matches!(
                record.update,
                FundingUpdate::Crediting { .. } | FundingUpdate::Collecting { .. }
            )
        })
    }

    /// What the core must follow to end the session, once the provider has
    /// handed the outcome over.
    pub fn settlement_due(&self) -> Option<Settlement> {
        if self.is_terminal() {
            return None;
        }
        match self.last_update()? {
            FundingUpdate::Delivered => Some(Settlement::TopUps(self.top_ups())),
            FundingUpdate::Collecting { payment_id, amount } => {
                Some(Settlement::Payment(*payment_id, *amount))
            }
            _ => None,
        }
    }

    /// End the session with what its top-ups claimed or its payment moved.
    /// Nothing moved fails it. Returns whether it changed.
    pub fn settle(&mut self, moved: u128, now_ms: u64) -> bool {
        if self.is_terminal() {
            return false;
        }
        self.stage = match (moved, self.direction) {
            (0, FundingDirection::In) => FundingStage::Failed {
                reason: FundingFailure::Other {
                    code: "not_claimed".to_string(),
                    message: "Nothing reached your balance".to_string(),
                },
                settled_at_ms: now_ms,
            },
            (0, FundingDirection::Out) => FundingStage::Failed {
                reason: FundingFailure::Other {
                    code: "payment_failed".to_string(),
                    message: "Your payment did not go through".to_string(),
                },
                settled_at_ms: now_ms,
            },
            (credited, FundingDirection::In) => FundingStage::Delivered {
                credited,
                settled_at_ms: now_ms,
            },
            (debited, FundingDirection::Out) => FundingStage::Released {
                debited,
                settled_at_ms: now_ms,
            },
        };
        true
    }

    /// Whether the host still has to hear of the session when funding
    /// resumes: it is in flight, or it ended and the host has not recorded
    /// the outcome.
    pub fn needs_handoff(&self) -> bool {
        !self.is_terminal() || !self.acknowledged
    }

    /// Cancel at the user's request: end the session if no provider serves
    /// it, otherwise ask the provider to stop, which it answers by reporting.
    /// Refused once the user's payment reached the provider, as getcash
    /// refuses once anything arrived.
    pub fn cancel(&mut self, now_ms: u64) -> CancelOutcome {
        if self.is_terminal() {
            return CancelOutcome::Refused;
        }
        if self.provider_id.is_none() {
            self.fail(FundingFailure::Cancelled, now_ms);
            return CancelOutcome::Ended;
        }
        let paid = self.funds_moving()
            || self
                .updates
                .iter()
                .any(|record| matches!(record.update, FundingUpdate::PaymentReceived { .. }));
        if paid {
            return CancelOutcome::Refused;
        }
        self.cancel_requested = true;
        CancelOutcome::Requested
    }

    /// Whether the session has ended.
    pub fn is_terminal(&self) -> bool {
        self.settled_at_ms().is_some()
    }

    /// When the session ended, if it has.
    pub fn settled_at_ms(&self) -> Option<u64> {
        match self.stage {
            FundingStage::Open => None,
            FundingStage::Failed { settled_at_ms, .. }
            | FundingStage::Delivered { settled_at_ms, .. }
            | FundingStage::Released { settled_at_ms, .. } => Some(settled_at_ms),
        }
    }

    /// Project the current stage onto the wire item subscribers receive.
    pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
        match (&self.stage, self.direction) {
            (FundingStage::Open, FundingDirection::In) => {
                HostFundingStatusSubscribeItem::AwaitingDeposit {
                    expires_at: (!self.funds_moving()).then_some(self.deadline_ms),
                }
            }
            (FundingStage::Open, FundingDirection::Out) => {
                HostFundingStatusSubscribeItem::AwaitingRelease
            }
            (FundingStage::Failed { reason, .. }, _) => HostFundingStatusSubscribeItem::Failed {
                reason: reason.clone(),
                moved: 0,
            },
            (FundingStage::Delivered { credited, .. }, _) => {
                HostFundingStatusSubscribeItem::Delivered {
                    credited: *credited,
                }
            }
            (FundingStage::Released { debited, .. }, _) => {
                HostFundingStatusSubscribeItem::Released { debited: *debited }
            }
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
        now_ms >= self.deadline_ms
            && !self.funds_moving()
            && self.fail(FundingFailure::Expired, now_ms)
    }
}

/// Order of an update within a session: a later update ranks higher, and a
/// top-up and a payment request rank where funds start moving.
fn update_rank(update: &FundingUpdate) -> u8 {
    match update {
        FundingUpdate::AwaitingPayment => 0,
        FundingUpdate::PaymentReceived { finalized: false } => 1,
        FundingUpdate::PaymentReceived { finalized: true } => 2,
        FundingUpdate::Converting => 3,
        FundingUpdate::Crediting { .. } | FundingUpdate::Collecting { .. } => 4,
        FundingUpdate::Delivered => 5,
        FundingUpdate::Failed { .. } => 6,
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
    /// The chosen provider is not a valid product id.
    #[display("invalid funding provider: {reason}")]
    InvalidProvider {
        /// Why the id was refused.
        reason: String,
    },
    /// The chosen quote is not one the core offered for this session, or it
    /// expired.
    #[display("unknown or expired funding quote {quote_id}")]
    UnknownQuote {
        /// The quote id the host named.
        quote_id: String,
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
    const PROVIDER: &str = "ramp.dot";

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

    fn served(direction: FundingDirection) -> FundingSession {
        let mut session = session(direction);
        assert!(session.assign(PROVIDER, None));
        session
    }

    fn reported(session: &mut FundingSession, updates: &[FundingUpdate]) {
        for update in updates {
            session
                .report(PROVIDER, update.clone(), NOW)
                .unwrap_or_else(|refusal| panic!("{update:?} refused: {refusal:?}"));
        }
    }

    // With no provider to ask, a cancel ends the session itself; once it
    // ended, its outcome stands.
    #[test]
    fn an_unserved_session_is_cancelled_once() {
        let mut open = session(FundingDirection::In);
        let mut ended = expired("fs_2", NOW);

        assert_eq!(
            (open.cancel(NOW + 1), open.wire_item(), ended.cancel(NOW + 1)),
            (
                CancelOutcome::Ended,
                HostFundingStatusSubscribeItem::Failed {
                    reason: FundingFailure::Cancelled,
                    moved: 0,
                },
                CancelOutcome::Refused
            )
        );
    }

    // The provider holds the user's money once it has seen the payment, so
    // only it can stop the session before that, and nobody can after.
    #[test]
    fn a_served_session_asks_its_provider_until_the_payment_arrives() {
        let mut before = served(FundingDirection::In);
        let mut after = served(FundingDirection::In);
        reported(&mut after, &[FundingUpdate::PaymentReceived { finalized: false }]);

        assert_eq!(
            (before.cancel(NOW), before.cancel_requested, before.is_terminal(), after.cancel(NOW)),
            (CancelOutcome::Requested, true, false, CancelOutcome::Refused)
        );
    }

    #[test]
    fn a_session_is_assigned_once() {
        let mut session = served(FundingDirection::In);

        assert!(!session.assign("other.dot", None));
        assert_eq!(session.provider_id.as_deref(), Some(PROVIDER));
    }

    // Reports come only from the assigned provider and only move forward, so
    // the stored timeline is one the host can draw in order.
    #[test]
    fn reports_come_from_the_provider_and_only_move_forward() {
        let mut session = served(FundingDirection::In);
        reported(&mut session, &[FundingUpdate::Converting]);

        assert_eq!(
            (
                session.report("other.dot", FundingUpdate::Converting, NOW),
                session.report(PROVIDER, FundingUpdate::AwaitingPayment, NOW),
                session.report(PROVIDER, FundingUpdate::Converting, NOW),
                session.report(PROVIDER, FundingUpdate::Collecting { payment_id: [1; 32], amount: 5 }, NOW),
                session.report(PROVIDER, FundingUpdate::Delivered, NOW),
            ),
            (
                Err(ReportRefusal::NotFound),
                Err(ReportRefusal::OutOfOrder),
                Err(ReportRefusal::OutOfOrder),
                Err(ReportRefusal::OutOfOrder),
                Err(ReportRefusal::OutOfOrder),
            )
        );
    }

    // A partial claim is retried with a new top-up, as getcash does in up to
    // three attempts; the host follows all of them once the provider is done.
    #[test]
    fn an_inbound_session_settles_from_every_top_up_it_names() {
        let mut session = served(FundingDirection::In);
        reported(
            &mut session,
            &[
                FundingUpdate::Crediting { top_up_id: [1; 32], amount: 70 },
                FundingUpdate::Crediting { top_up_id: [2; 32], amount: 30 },
            ],
        );
        let repeated = session.report(PROVIDER, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 30 }, NOW);
        let due_before_done = session.settlement_due();
        reported(&mut session, &[FundingUpdate::Delivered]);

        assert_eq!(
            (repeated, due_before_done, session.settlement_due()),
            (
                Err(ReportRefusal::OutOfOrder),
                None,
                Some(Settlement::TopUps(vec![([1; 32], 70), ([2; 32], 30)]))
            )
        );
    }

    // Once a top-up or payment started, the money is in motion: the provider
    // cannot fail the session and the deadline does not end it.
    #[test]
    fn moving_funds_are_settled_by_the_host_not_the_provider_or_the_clock() {
        let mut session = served(FundingDirection::Out);
        reported(&mut session, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 40 }]);

        let failed = session.report(PROVIDER, FundingUpdate::Failed { reason: FundingFailure::ProviderTimeout }, NOW);
        let expired = session.expire_if_due(NOW + SESSION_WINDOW_MS);

        assert_eq!(
            (failed, expired, session.settlement_due()),
            (Err(ReportRefusal::OutOfOrder), false, Some(Settlement::Payment([3; 32], 40)))
        );
    }

    #[test]
    fn settling_records_what_moved_and_nothing_fails_the_session() {
        let mut credited = served(FundingDirection::In);
        let mut debited = served(FundingDirection::Out);
        let mut unclaimed = served(FundingDirection::In);

        credited.settle(90, NOW);
        debited.settle(40, NOW);
        unclaimed.settle(0, NOW);

        assert_eq!(
            (credited.wire_item(), debited.wire_item(), unclaimed.wire_item()),
            (
                HostFundingStatusSubscribeItem::Delivered { credited: 90 },
                HostFundingStatusSubscribeItem::Released { debited: 40 },
                HostFundingStatusSubscribeItem::Failed {
                    reason: FundingFailure::Other {
                        code: "not_claimed".to_string(),
                        message: "Nothing reached your balance".to_string(),
                    },
                    moved: 0,
                },
            )
        );
    }

    #[test]
    fn a_provider_failure_ends_the_session_and_is_kept() {
        let mut session = served(FundingDirection::In);
        let failure = FundingUpdate::Failed { reason: FundingFailure::VerificationRefused };

        reported(&mut session, &[FundingUpdate::AwaitingPayment, failure.clone()]);

        assert_eq!(
            (session.wire_item(), session.last_update()),
            (
                HostFundingStatusSubscribeItem::Failed {
                    reason: FundingFailure::VerificationRefused,
                    moved: 0,
                },
                Some(&failure)
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
