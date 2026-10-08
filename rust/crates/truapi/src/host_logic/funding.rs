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
    FundingAssignment, FundingDeposit, FundingDirection, FundingFailure, FundingPayout, FundingQuote, FundingRail, FundingReceived, FundingUpdate,
    HostFundingStatusSubscribeItem,
};

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has not recorded yet the core keeps,
/// newest first, so a host that never records them cannot grow the store
/// without bound.
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
    /// What the user chose the provider on: its quote, and the rail and
    /// asset it priced. `None` until chosen, or when chosen unquoted.
    pub choice: Option<FundingChoice>,
    /// The provider's own state for the session, opaque to the host. Kept
    /// only while the session is open.
    pub saved: Option<Vec<u8>>,
}

/// The most bytes of provider state a session keeps.
pub const MAX_SAVED_BYTES: u32 = 4096;

/// The quote a session's provider was chosen on, with the rail and asset it
/// priced.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingChoice {
    /// The quote.
    pub quote: FundingQuote,
    /// The rail the user pays or is paid by.
    pub rail: FundingRail,
    /// The asset symbol the user pays with or receives.
    pub asset: String,
}

/// A session's progress as a host draws it: the steps for its direction and
/// rail, each with when it was reached.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingProgress {
    /// The steps in order; a step reached later fills in any skipped
    /// before it with its time.
    pub steps: Vec<FundingProgressStep>,
    /// When the session failed, if it did.
    pub failed_at_ms: Option<u64>,
    /// The provider's transaction id, from its latest `Details`.
    pub transaction_id: Option<String>,
    /// The provider's reference, from its latest `Details`.
    pub reference: Option<String>,
    /// Where and what the user pays, from the provider's latest `Deposit`.
    pub deposit: Option<FundingDeposit>,
    /// What arrived when it differs from what was asked, from the latest
    /// `PaymentReceived`.
    pub mismatch: Option<FundingReceived>,
    /// Whether the provider is retrying: it named another top-up after a
    /// first one claimed only part of the amount.
    pub retrying: bool,
    /// How the payout went, once the provider reported it on a released
    /// outbound session.
    pub payout: Option<FundingPayout>,
}

/// One step of a session's progress.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingProgressStep {
    /// The step.
    pub step: FundingStep,
    /// When it was reached, in Unix milliseconds.
    pub reached_at_ms: Option<u64>,
}

/// A step a session goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingStep {
    /// The session was opened.
    Started,
    /// The provider saw the user's payment, or asked the user to pay it.
    Payment,
    /// The payment can no longer be reversed. Bank and crypto only.
    Approved,
    /// The provider is converting to or from the balance asset.
    Conversion,
    /// The funds reached the user's balance.
    Added,
    /// The funds left the user's balance.
    Sent,
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

/// Why a provider's state was not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRefusal {
    /// No open session with this id is assigned to the provider.
    NotFound,
    /// Larger than [`MAX_SAVED_BYTES`].
    TooLarge,
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
            choice: None,
            saved: None,
        }
    }

    /// Assign the session to the provider the user chose, on the quote they
    /// chose it by. Returns whether it was open and not yet assigned.
    pub fn assign(&mut self, provider_id: &str, choice: Option<FundingChoice>) -> bool {
        if self.is_terminal() || self.provider_id.is_some() {
            return false;
        }
        self.provider_id = Some(provider_id.to_string());
        self.choice = choice;
        true
    }

    /// The session as the provider serving it receives it.
    pub fn assignment(&self) -> FundingAssignment {
        FundingAssignment {
            intent: self.intent.clone(),
            direction: self.direction,
            amount: self.amount,
            expires_at: self.deadline_ms,
            last_update: self.last_step().cloned(),
            quote: self.choice.as_ref().map(|choice| choice.quote.clone()),
            saved: self.saved.clone(),
        }
    }

    /// The provider's most recent update.
    pub fn last_update(&self) -> Option<&FundingUpdate> {
        self.updates.last().map(|record| &record.update)
    }

    /// The provider's most recent update that moves the session on, which
    /// `Details` does not.
    pub fn last_step(&self) -> Option<&FundingUpdate> {
        self.updates
            .iter()
            .rev()
            .map(|record| &record.update)
            .find(|update| update_rank(update).is_some())
    }

    /// The session's progress for its direction and rail.
    pub fn progress(&self) -> FundingProgress {
        let steps: &[FundingStep] = match (self.direction, self.choice.as_ref().map(|choice| choice.rail)) {
            (FundingDirection::Out, _) => &[FundingStep::Started, FundingStep::Payment, FundingStep::Sent],
            (FundingDirection::In, Some(FundingRail::Card) | None) => &[
                FundingStep::Started,
                FundingStep::Payment,
                FundingStep::Conversion,
                FundingStep::Added,
            ],
            (FundingDirection::In, Some(FundingRail::Bank | FundingRail::Crypto)) => &[
                FundingStep::Started,
                FundingStep::Payment,
                FundingStep::Approved,
                FundingStep::Conversion,
                FundingStep::Added,
            ],
        };
        let first = |matches: fn(&FundingUpdate) -> bool| {
            self.updates
                .iter()
                .find(|record| matches(&record.update))
                .map(|record| record.at_ms)
        };
        let reached = |step: FundingStep| match step {
            FundingStep::Started => Some(self.opened_at_ms),
            FundingStep::Payment => first(|update| {
                matches!(
                    update,
                    FundingUpdate::PaymentReceived { .. } | FundingUpdate::Collecting { .. }
                )
            }),
            FundingStep::Approved => {
                first(|update| matches!(update, FundingUpdate::PaymentReceived { finalized: true, .. }))
            }
            FundingStep::Conversion => first(|update| {
                matches!(update, FundingUpdate::Converting | FundingUpdate::Crediting { .. })
            }),
            FundingStep::Added => match self.stage {
                FundingStage::Delivered { settled_at_ms, .. } => Some(settled_at_ms),
                _ => None,
            },
            FundingStep::Sent => match self.stage {
                FundingStage::Released { settled_at_ms, .. } => Some(settled_at_ms),
                _ => None,
            },
        };
        let mut times: Vec<Option<u64>> = steps.iter().map(|step| reached(*step)).collect();
        // A provider may skip a step; one reached later says the earlier
        // ones were passed by then.
        for index in (0..times.len().saturating_sub(1)).rev() {
            if times[index].is_none() {
                times[index] = times[index + 1];
            }
        }
        let latest = |field: fn(&FundingUpdate) -> Option<&String>| {
            self.updates
                .iter()
                .rev()
                .find_map(|record| field(&record.update))
                .cloned()
        };
        FundingProgress {
            steps: steps
                .iter()
                .zip(times)
                .map(|(step, reached_at_ms)| FundingProgressStep {
                    step: *step,
                    reached_at_ms,
                })
                .collect(),
            failed_at_ms: match self.stage {
                FundingStage::Failed { settled_at_ms, .. } => Some(settled_at_ms),
                _ => None,
            },
            transaction_id: latest(|update| match update {
                FundingUpdate::Details { transaction_id, .. } => transaction_id.as_ref(),
                _ => None,
            }),
            reference: latest(|update| match update {
                FundingUpdate::Details { reference, .. } => reference.as_ref(),
                _ => None,
            }),
            deposit: self.updates.iter().rev().find_map(|record| match &record.update {
                FundingUpdate::Deposit { deposit } => Some(deposit.clone()),
                _ => None,
            }),
            mismatch: self
                .updates
                .iter()
                .rev()
                .find_map(|record| match &record.update {
                    FundingUpdate::PaymentReceived { mismatch, .. } => Some(mismatch.clone()),
                    _ => None,
                })
                .flatten(),
            retrying: !self.is_terminal() && self.top_ups().len() > 1,
            payout: self.payout().cloned(),
        }
    }

    /// Store `update` from `provider_id`. A `Failed` update ends the session.
    pub fn report(
        &mut self,
        provider_id: &str,
        update: FundingUpdate,
        now_ms: u64,
    ) -> Result<(), ReportRefusal> {
        if self.provider_id.as_deref() != Some(provider_id) {
            return Err(ReportRefusal::NotFound);
        }
        if matches!(update, FundingUpdate::Payout { .. }) {
            return self.report_payout(update, now_ms);
        }
        if self.is_terminal() {
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

    /// Keep `state` from `provider_id` in place of what it saved before.
    pub fn save(&mut self, provider_id: &str, state: Vec<u8>) -> Result<(), SaveRefusal> {
        if self.is_terminal() || self.provider_id.as_deref() != Some(provider_id) {
            return Err(SaveRefusal::NotFound);
        }
        if state.len() > MAX_SAVED_BYTES as usize {
            return Err(SaveRefusal::TooLarge);
        }
        self.saved = Some(state);
        Ok(())
    }

    /// Store the provider's payout outcome on a released outbound session the
    /// host has not recorded yet. Only one is kept.
    fn report_payout(&mut self, update: FundingUpdate, now_ms: u64) -> Result<(), ReportRefusal> {
        let released = matches!(self.stage, FundingStage::Released { .. });
        if !released || self.acknowledged {
            return Err(ReportRefusal::NotFound);
        }
        if self.payout().is_some() {
            return Err(ReportRefusal::OutOfOrder);
        }
        self.updates.push(FundingUpdateRecord {
            update,
            at_ms: now_ms,
        });
        Ok(())
    }

    /// The provider's payout outcome, once it reported one.
    pub fn payout(&self) -> Option<&FundingPayout> {
        self.updates.iter().find_map(|record| match &record.update {
            FundingUpdate::Payout { outcome } => Some(outcome),
            _ => None,
        })
    }

    /// Whether `update` may come next. Updates only move forward, a session
    /// may credit through a few top-ups, and once funds move the provider
    /// can no longer fail it: the top-ups or payment decide.
    fn follows(&self, update: &FundingUpdate) -> bool {
        let fits_direction = match update {
            FundingUpdate::Collecting { .. } => self.direction == FundingDirection::Out,
            FundingUpdate::Failed { .. } | FundingUpdate::Details { .. } => true,
            _ => self.direction == FundingDirection::In,
        };
        let last = self.last_step().and_then(update_rank);
        let top_ups = self.top_ups();
        fits_direction
            && match update {
                FundingUpdate::Details { .. } => true,
                FundingUpdate::Failed { .. } | FundingUpdate::Deposit { .. } => !self.funds_moving(),
                FundingUpdate::Crediting { top_up_id, .. } => {
                    last <= update_rank(update)
                        && top_ups.len() < TOP_UP_LIMIT
                        && top_ups.iter().all(|(id, _)| id != top_up_id)
                }
                FundingUpdate::Delivered => last < update_rank(update) && !top_ups.is_empty(),
                _ => last < update_rank(update),
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
        match self.last_step()? {
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
            (FundingStage::Open, _) => HostFundingStatusSubscribeItem::InProgress {
                expires_at: self.can_expire().then_some(self.deadline_ms),
            },
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

    /// Expire the session if it is still open at its deadline with no
    /// provider assigned. An assigned session waits for its provider or a
    /// cancel: a bank transfer can take days. Returns whether it expired.
    pub fn expire_if_due(&mut self, now_ms: u64) -> bool {
        now_ms >= self.deadline_ms && self.can_expire() && self.fail(FundingFailure::Expired, now_ms)
    }

    /// Whether the session's deadline still applies: it is open and no
    /// provider has taken it.
    pub fn can_expire(&self) -> bool {
        !self.is_terminal() && self.provider_id.is_none()
    }
}

/// Order of an update within a session: a later update ranks higher, and a
/// top-up and a payment request rank where funds start moving. `Deposit`
/// and `Details` have no place in the order.
fn update_rank(update: &FundingUpdate) -> Option<u8> {
    Some(match update {
        FundingUpdate::AwaitingPayment => 0,
        FundingUpdate::PaymentReceived { finalized: false, .. } => 1,
        FundingUpdate::PaymentReceived { finalized: true, .. } => 2,
        FundingUpdate::Converting => 3,
        FundingUpdate::Crediting { .. } | FundingUpdate::Collecting { .. } => 4,
        FundingUpdate::Delivered => 5,
        FundingUpdate::Failed { .. } => 6,
        FundingUpdate::Deposit { .. }
        | FundingUpdate::Details { .. }
        | FundingUpdate::Payout { .. } => return None,
    })
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

/// The sessions worth keeping: every open one, then the ended ones the host
/// has not recorded yet, newest first up to a fixed bound. The host owns the
/// history; a session it has recorded is no longer kept.
pub fn retained(sessions: impl IntoIterator<Item = FundingSession>) -> Vec<FundingSession> {
    let (mut open, mut unrecorded): (Vec<_>, Vec<_>) = sessions
        .into_iter()
        .filter(FundingSession::needs_handoff)
        .partition(|session| !session.is_terminal());
    unrecorded.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
    unrecorded.truncate(UNACKNOWLEDGED_LIMIT);
    open.append(&mut unrecorded);
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

    // A product only waits for the outcome; the host draws the steps, so an
    // open session reads the same whichever way value moves.
    #[test]
    fn an_open_session_reads_in_progress_until_its_deadline() {
        let in_progress = HostFundingStatusSubscribeItem::InProgress {
            expires_at: Some(NOW + SESSION_WINDOW_MS),
        };
        assert_eq!(
            (
                session(FundingDirection::In).wire_item(),
                session(FundingDirection::Out).wire_item(),
            ),
            (in_progress.clone(), in_progress)
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

    // A bank transfer can take days, so once a provider has taken a session it
    // waits for the provider or a cancel instead of failing on day two, and
    // products are no longer shown a deadline.
    #[test]
    fn only_a_session_no_provider_has_taken_expires() {
        let mut assigned = served(FundingDirection::In);

        assert_eq!(
            (assigned.expire_if_due(NOW + SESSION_WINDOW_MS), assigned.wire_item()),
            (false, HostFundingStatusSubscribeItem::InProgress { expires_at: None })
        );
    }

    #[test]
    fn an_ended_session_keeps_its_first_outcome() {
        let mut session = expired("fs_1", NOW);

        assert!(!session.fail(FundingFailure::Cancelled, NOW + 1));
        assert_eq!(session, expired("fs_1", NOW));
    }

    // The host owns the history: a session it recorded is dropped, one it has
    // not recorded is kept for it, newest first and not without bound, and
    // open sessions always come first.
    #[test]
    fn the_core_keeps_only_open_and_unrecorded_sessions() {
        let ended = |index: usize, acknowledged| FundingSession {
            acknowledged,
            ..expired(&format!("fs_s{index}"), NOW + index as u64)
        };
        let open = session(FundingDirection::Out);
        let recorded = (0..3).map(|index| ended(index, true));
        let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

        let kept: Vec<String> = retained(recorded.chain(unrecorded).chain([open]))
            .into_iter()
            .map(|session| session.intent)
            .collect();

        let newest_unrecorded = (101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
        assert_eq!(
            kept,
            std::iter::once("fs_1".to_string())
                .chain(newest_unrecorded)
                .collect::<Vec<_>>()
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
        reported(&mut after, &[FundingUpdate::PaymentReceived { finalized: false, mismatch: None }]);

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

    fn chosen(direction: FundingDirection, rail: FundingRail) -> FundingSession {
        let mut session = session(direction);
        let quote = FundingQuote {
            quote_id: "q1".to_string(),
            send_amount: 100,
            receive_amount: 99,
            provider_fee: 1,
            network_fee: 0,
            eta_secs: None,
            expires_at: None,
        };
        let choice = FundingChoice { quote, rail, asset: "EUR".to_string() };
        assert!(session.assign(PROVIDER, Some(choice)));
        session
    }

    fn report_at(session: &mut FundingSession, update: FundingUpdate, at_ms: u64) {
        session.report(PROVIDER, update, at_ms).expect("reported");
    }

    fn steps(progress: &FundingProgress) -> Vec<(FundingStep, Option<u64>)> {
        progress.steps.iter().map(|step| (step.step, step.reached_at_ms)).collect()
    }

    // A bank payment is approved once it can no longer be reversed, a card
    // payment has no such step, and a step the provider skipped takes the
    // time of the first later one, so the bar never shows a gap behind it.
    #[test]
    fn progress_follows_the_rail_and_fills_skipped_steps() {
        let mut bank = chosen(FundingDirection::In, FundingRail::Bank);
        report_at(&mut bank, FundingUpdate::PaymentReceived { finalized: false, mismatch: None }, NOW + 1);
        report_at(&mut bank, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 }, NOW + 3);
        let mut card = chosen(FundingDirection::In, FundingRail::Card);
        report_at(&mut card, FundingUpdate::Converting, NOW + 2);
        let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
        report_at(&mut out, FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 }, NOW + 1);
        assert!(out.settle(100, NOW + 4));

        assert_eq!(
            (steps(&bank.progress()), steps(&card.progress()), steps(&out.progress())),
            (
                vec![
                    (FundingStep::Started, Some(NOW)),
                    (FundingStep::Payment, Some(NOW + 1)),
                    (FundingStep::Approved, Some(NOW + 3)),
                    (FundingStep::Conversion, Some(NOW + 3)),
                    (FundingStep::Added, None),
                ],
                vec![
                    (FundingStep::Started, Some(NOW)),
                    (FundingStep::Payment, Some(NOW + 2)),
                    (FundingStep::Conversion, Some(NOW + 2)),
                    (FundingStep::Added, None),
                ],
                vec![
                    (FundingStep::Started, Some(NOW)),
                    (FundingStep::Payment, Some(NOW + 1)),
                    (FundingStep::Sent, Some(NOW + 4)),
                ],
            )
        );
    }

    // The provider's references can come at any point, even while funds
    // move, without changing which step comes next or what settles the
    // session; the latest of each is shown.
    #[test]
    fn details_sit_outside_the_order_of_steps() {
        let mut session = chosen(FundingDirection::In, FundingRail::Bank);
        let crediting = FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 };
        reported(
            &mut session,
            &[
                FundingUpdate::Details { transaction_id: Some("tx-1".into()), reference: Some("REF".into()) },
                crediting.clone(),
                FundingUpdate::Details { transaction_id: Some("tx-2".into()), reference: None },
                FundingUpdate::Delivered,
            ],
        );
        let progress = session.progress();

        assert_eq!(
            (
                session.assignment().last_update,
                session.settlement_due(),
                progress.transaction_id,
                progress.reference,
            ),
            (
                Some(FundingUpdate::Delivered),
                Some(Settlement::TopUps(vec![([1; 32], 100)])),
                Some("tx-2".to_string()),
                Some("REF".to_string()),
            )
        );
    }

    fn usdt_deposit(amount: u128) -> FundingDeposit {
        FundingDeposit::Crypto {
            address: "0xdeposit".to_string(),
            network: "Ethereum".to_string(),
            asset: "USDT".to_string(),
            amount,
            decimals: 6,
            exact: true,
            uri: None,
            expires_at: Some(NOW + 1_000),
        }
    }

    // A short payment, as getcash handles it: the provider says what arrived
    // and asks for the rest at the same address; the host shows the latest
    // instructions. Once funds move, or for a withdrawal, there is nothing
    // left for the user to pay.
    #[test]
    fn a_short_payment_asks_for_the_rest_until_funds_move() {
        let mut session = served(FundingDirection::In);
        let short = FundingReceived { asset: "USDT".to_string(), amount: 60 };
        reported(
            &mut session,
            &[
                FundingUpdate::Deposit { deposit: usdt_deposit(100) },
                FundingUpdate::PaymentReceived { finalized: false, mismatch: Some(short.clone()) },
                FundingUpdate::Deposit { deposit: usdt_deposit(40) },
            ],
        );
        let progress = session.progress();
        reported(&mut session, &[FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 }]);
        let after_funds_move = session.report(PROVIDER, FundingUpdate::Deposit { deposit: usdt_deposit(1) }, NOW);
        let withdrawal = served(FundingDirection::Out).report(PROVIDER, FundingUpdate::Deposit { deposit: usdt_deposit(1) }, NOW);

        assert_eq!(
            (progress.deposit, progress.mismatch, after_funds_move, withdrawal),
            (
                Some(usdt_deposit(40)),
                Some(short),
                Err(ReportRefusal::OutOfOrder),
                Err(ReportRefusal::OutOfOrder),
            )
        );
    }

    // The user's CASH has already left when a payout fails, so the session
    // stays Released and the provider's payout outcome is kept beside it, once,
    // and only until the host records the session.
    #[test]
    fn a_released_session_takes_one_payout_outcome_until_recorded() {
        let mut released = served(FundingDirection::Out);
        reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
        assert!(released.settle(100, NOW + 1));
        let failed = FundingPayout::Failed { reason: "bank rejected the transfer".to_string() };
        let first = released.report(PROVIDER, FundingUpdate::Payout { outcome: failed.clone() }, NOW + 2);
        let second = released.report(PROVIDER, FundingUpdate::Payout { outcome: FundingPayout::PaidOut }, NOW + 3);
        let mut in_flight = served(FundingDirection::Out);
        let too_early = in_flight.report(PROVIDER, FundingUpdate::Payout { outcome: FundingPayout::PaidOut }, NOW);
        let mut recorded = released.clone();
        recorded.acknowledged = true;
        recorded.updates.retain(|record| !matches!(record.update, FundingUpdate::Payout { .. }));
        let after_recording = recorded.report(PROVIDER, FundingUpdate::Payout { outcome: FundingPayout::PaidOut }, NOW + 4);

        assert_eq!(
            (first, second, too_early, after_recording, released.progress().payout),
            (
                Ok(()),
                Err(ReportRefusal::OutOfOrder),
                Err(ReportRefusal::NotFound),
                Err(ReportRefusal::NotFound),
                Some(failed),
            )
        );
    }

    // A second top-up after a partial claim is the provider retrying, which
    // the host shows as "Retrying your payment"; one top-up is not.
    #[test]
    fn a_second_top_up_reads_as_retrying() {
        let mut session = served(FundingDirection::In);
        reported(&mut session, &[FundingUpdate::Crediting { top_up_id: [1; 32], amount: 60 }]);
        let after_one = session.progress().retrying;
        reported(&mut session, &[FundingUpdate::Crediting { top_up_id: [2; 32], amount: 40 }]);

        assert_eq!((after_one, session.progress().retrying), (false, true));
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
