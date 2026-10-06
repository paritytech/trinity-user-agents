//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death through
//! [`CoreStorageKey::FundingSessions`], and is re-attached by
//! `Funding::status_subscribe`. A session always terminates, because the core
//! expires it on its own clock.

use std::collections::BTreeMap;

use parity_scale_codec::{Decode, Encode};
use tracing::warn;
use truapi::latest::{
    FundingDirection, FundingFailure, HostFundingStatusSubscribeItem, HostPaymentStatusSubscribeItem,
};

use crate::host_logic::entropy::{ProductEntropyError, derive_product_entropy};
use crate::host_logic::product_account::{ProductAccountError, derive_root_keypair_from_entropy};
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
    /// Where an inbound session's provider delivers, once the source is
    /// known.
    pub deposit: Option<FundingDeposit>,
    /// The deposit quoted to credit `amount`, frozen so the provider is held
    /// to the figure it was given rather than one re-priced later.
    pub quote: Option<DepositQuote>,
    /// CASH the conversion landed on People, in payment balance units, once
    /// it has: what crediting claims from, and what stays stranded on the
    /// account when less is credited.
    pub landed: Option<u128>,
    /// Where an outbound session's payment goes, once its destination is
    /// known.
    pub withdrawal: Option<FundingWithdrawal>,
    /// When each step in flight was first reached, in the order reached.
    /// The session starts at `opened_at_ms` and ends when its stage says.
    pub stamps: Vec<FundingStamp>,
    /// Whether the host has recorded the session's outcome in its own
    /// history. An ended session is handed to the host until it has.
    pub acknowledged: bool,
}

/// The account an outbound session's payment goes to, and the payment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingWithdrawal {
    /// Where the funds go from Asset Hub, as in the account label.
    pub destination_id: String,
    /// Account number for `destination_id`.
    pub number: u32,
    /// Public key of the withdrawal account on People and Asset Hub.
    pub account: [u8; 32],
    /// Which payment attempt is asked for, from 0; its id is
    /// [`funding_attempt_id`] of the account.
    pub attempt: u8,
    /// When that attempt was asked for, in Unix milliseconds.
    pub since_ms: u64,
    /// Whether the host reported the payment under way or done, after which
    /// the session no longer expires or cancels, as getcash holds a taken
    /// payment.
    pub taken: bool,
}

/// The id of attempt `attempt` on `account`, as getcash numbers its top-ups
/// and payments: the account itself first, then
/// `blake2_256(account ‖ u32le attempt)`.
pub fn funding_attempt_id(account: &[u8; 32], attempt: u8) -> [u8; 32] {
    if attempt == 0 {
        return *account;
    }
    sp_crypto_hashing::blake2_256(&[account.as_slice(), &u32::from(attempt).to_le_bytes()].concat())
}

/// What the host said of a withdrawal's payment, and for which attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReading {
    /// The attempt whose id was asked about.
    pub attempt: u8,
    /// The answer.
    pub word: PaymentWord,
}

/// The host's answer about one payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentWord {
    /// The host did not answer in time.
    Unanswered,
    /// The host holds no payment under the id.
    NotFound,
    /// The payment's status.
    Said(HostPaymentStatusSubscribeItem),
}

/// How long a requested payment may go untaken before the session expires,
/// as getcash's payment window.
pub const PAYMENT_WINDOW_MS: u64 = 30 * 60 * 1_000;
/// Code a session fails with when its payment did not go through.
const PAYMENT_FAILED: &str = "payment_failed";

/// Where a session is, for the host's progress and history views: the
/// stage, with an inbound session's deposit account and submission folded
/// in. Each rail's markers are drawn from these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingStep {
    /// Opened; nothing chosen yet.
    Started,
    /// An outbound session asked the user to pay into its withdrawal
    /// account.
    AwaitingPayment,
    /// The user's CASH is on the withdrawal account.
    Paid,
    /// An inbound session has a deposit account and waits for the payment.
    AwaitingDeposit,
    /// The deposit arrived.
    DepositSeen,
    /// The conversion to CASH is on its way.
    Converting,
    /// The converted funds reached the other chain: an on-ramp's CASH on
    /// People, a withdrawal's PAS on Asset Hub.
    Landed,
    /// The host's top-up is claiming the CASH.
    Claiming,
    /// The CASH is in the user's balance.
    Settled,
    /// The session ran out of time.
    Expired,
    /// The session was cancelled.
    Cancelled,
    /// The session ended without success.
    Failed,
}

/// When a session first reached a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingStamp {
    /// The step.
    pub step: FundingStep,
    /// When it was first reached, in Unix milliseconds.
    pub at_ms: u64,
}

/// The route for a deposit and what it must deliver to credit a session's
/// amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct DepositQuote {
    /// Asset the deposit is quoted in.
    pub asset: DepositAsset,
    /// How the deposit becomes CASH.
    pub route: ConversionRoute,
    /// Least deposit, in the asset's units.
    pub deposit: u128,
}

/// Asset Hub asset a deposit arrives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum DepositAsset {
    /// The relay chain's native token.
    Native,
    /// An `Assets` pallet asset.
    Asset(u32),
}

/// What an inbound session's provider delivers, once it is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct DepositRequest {
    /// Deposit source, as in the account label, such as `usdt-assethub`.
    pub source_id: String,
    /// Asset the provider delivers.
    pub asset: DepositAsset,
    /// Balance at which the deposit counts as delivered, in `asset` units.
    pub expected: u128,
}

/// The account an inbound session watches and what it waits for.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct FundingDeposit {
    /// Deposit source, as in the account label.
    pub source_id: String,
    /// Account number for `source_id`; the refund account shares it.
    pub number: u32,
    /// Asset the provider delivers.
    pub asset: DepositAsset,
    /// Public key of the deposit account, kept so the watch needs no signing
    /// session.
    pub account: [u8; 32],
    /// Balance at which the deposit counts as delivered, in `asset` units.
    pub expected: u128,
    /// How the deposit becomes CASH on People.
    pub route: ConversionRoute,
    /// CASH the session asks to credit, which a swap must not land below.
    pub target: Option<u128>,
    /// What the deposit account held at the last reading, each asset with a
    /// balance, the native token last.
    pub holdings: Vec<DepositHolding>,
}

/// One asset on a deposit account and its balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct DepositHolding {
    /// The asset.
    pub asset: DepositAsset,
    /// Its balance, in the asset's units.
    pub balance: u128,
}

/// What arrived on a deposit account that does not match what was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum DepositMismatch {
    /// Less of the requested asset than the deposit was quoted at.
    Short {
        /// The requested asset.
        asset: DepositAsset,
        /// What arrived of it.
        amount: u128,
    },
    /// Another asset arrived while the requested one falls short.
    WrongAsset {
        /// The asset that arrived.
        asset: DepositAsset,
        /// How much.
        amount: u128,
    },
}

impl FundingDeposit {
    /// The balance of `asset` at the last reading.
    pub fn held(&self, asset: DepositAsset) -> u128 {
        self.holdings
            .iter()
            .find(|holding| holding.asset == asset)
            .map_or(0, |holding| holding.balance)
    }

    /// What arrived that does not match the request, as getcash judges it:
    /// nothing once the requested asset reaches `gate`; otherwise another
    /// asset that arrived, the native token only if nothing else did, since a
    /// little of it sent to pay fees must not stand in for the stablecoin;
    /// otherwise less of the requested asset than asked.
    fn mismatch_against(&self, gate: u128) -> Option<DepositMismatch> {
        let held = self.held(self.asset);
        if held >= gate {
            return None;
        }
        let stray = self
            .holdings
            .iter()
            .find(|holding| holding.asset != self.asset && holding.balance > 0);
        match stray {
            Some(holding) => Some(DepositMismatch::WrongAsset {
                asset: holding.asset,
                amount: holding.balance,
            }),
            None => (held > 0).then_some(DepositMismatch::Short {
                asset: self.asset,
                amount: held,
            }),
        }
    }
}

/// How a deposit becomes CASH on People, fixed when its account is assigned
/// so a later change on chain cannot switch it mid-session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum ConversionRoute {
    /// The deposit is CASH already: teleport it.
    Teleport,
    /// The deposit is a stablecoin the PSM mints CASH against: mint, then
    /// teleport.
    Psm {
        /// Minting fee the route was chosen at, in parts per million.
        fee_ppm: u32,
    },
    /// The deposit is the native token, or a stablecoin the PSM cannot
    /// serve: swap it to CASH through the asset-conversion pools, then
    /// teleport.
    Pool,
}

/// A conversion transaction handed to Asset Hub, with what tells whether it
/// worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct ConversionSubmission {
    /// The deposit account's nonce the transaction was signed at.
    pub nonce: u32,
    /// When it was submitted, in Unix milliseconds.
    pub submitted_at_ms: u64,
    /// Last Asset Hub block its mortal era admits it in.
    pub valid_until_block: u64,
    /// CASH the account held on People before it was submitted.
    pub people_before: u128,
    /// Least CASH the conversion lands on People.
    pub landing: u128,
    /// Deposit the transaction takes from the account on Asset Hub.
    pub spent: u128,
}

/// Refusals before a conversion gives up, as getcash holds a mint the PSM
/// keeps refusing.
const MAX_CONVERSION_REFUSALS: u8 = 3;
/// Top-up attempts before crediting settles for what they claimed.
const MAX_CLAIM_ATTEMPTS: u8 = 3;
/// How long a session that ended with its deposit recoverable keeps being
/// read, as getcash watches a payment: funds that arrive late, or stay after
/// a refused conversion, can still be converted.
pub const LATE_WATCH_MS: u64 = 72 * 60 * 60 * 1_000;
/// Code a session fails with when its conversion was refused.
const CONVERSION_REFUSED: &str = "conversion_refused";
/// Code a session fails with when the PSM would not mint its deposit.
const CONVERSION_HELD: &str = "conversion_held";
/// Code a session fails with when its top-ups claimed nothing.
const CREDIT_UNCLAIMED: &str = "credit_unclaimed";

/// Why a session cannot be cancelled, as getcash refuses a cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum CancelRefusal {
    /// The deposit arrived or the session went further, or it already ended
    /// other than by expiring.
    #[display("the session is already under way or over")]
    Underway,
    /// Something is on the deposit account, which a cancel would strand.
    #[display("funds are on the deposit account")]
    FundsArrived,
}

/// Why a failed session cannot be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum RetryRefusal {
    /// It did not fail, or its funds are not where a retry can reach them.
    #[display("the session has nothing to retry")]
    NotResumable,
    /// Nothing of the deposit's asset is on the deposit account.
    #[display("the deposit account holds none of the deposit")]
    NothingHeld,
}

/// Why a deposit could not be accepted as it arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum AcceptRefusal {
    /// The session is converting or done, or ended for good.
    #[display("the session is not awaiting or holding a deposit")]
    NotAcceptable,
    /// Nothing that differs from the request is on the deposit account.
    #[display("nothing other than the requested deposit is on the account")]
    NothingArrived,
    /// The asset is not the one that arrived in place of the request.
    #[display("that asset is not what arrived in place of the request")]
    NotMismatched,
    /// The deposit account changed while the route was being chosen.
    #[display("the deposit account changed; read it again")]
    Changed,
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
    /// Inbound: the deposit arrived and is being converted.
    Converting {
        /// Balance of the deposit account when it was seen, in its asset's
        /// units.
        deposited: u128,
        /// Dry runs the chain has refused so far.
        refusals: u8,
        /// The conversion transaction, once one is on its way.
        submission: Option<ConversionSubmission>,
    },
    /// Inbound: CASH landed on the deposit account on People and awaits
    /// crediting.
    Converted,
    /// Inbound: the host's top-up is claiming the landed CASH into the
    /// user's balance.
    Crediting {
        /// What the top-ups have claimed so far.
        progress: CreditProgress,
    },
    /// Outbound: the user's CASH is on the withdrawal account on People,
    /// being moved to Asset Hub.
    Paid {
        /// CASH the account received, in payment balance units.
        paid: u128,
        /// Withdrawal transactions included on People that failed so far.
        rejections: u8,
        /// When sizing or People's transaction pool started refusing the
        /// next transaction, if they still do, in Unix milliseconds.
        refused_since_ms: Option<u64>,
        /// The withdrawal transaction on its way, if one is.
        submission: Option<WithdrawSubmission>,
    },
    /// Outbound: the withdrawal's PAS is on the withdrawal account on Asset
    /// Hub, to be paid out.
    Withdrawn {
        /// PAS that landed, in planck.
        landed: u128,
    },
    /// Inbound terminal success: the CASH is in the user's balance.
    Delivered {
        /// Amount credited, in payment balance units.
        credited: u128,
        /// When it was credited, in Unix milliseconds.
        settled_at_ms: u64,
    },
    /// Ended without success.
    Failed {
        /// Why it ended.
        reason: FundingFailure,
        /// When it ended, in Unix milliseconds.
        settled_at_ms: u64,
        /// Where a retry picks up, when the funds are still on the session's
        /// accounts.
        resume: Option<FundingResume>,
    },
}

/// Where a retry of a failed session picks up, as getcash re-arms a held or
/// unclaimed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingResume {
    /// The deposit is still on Asset Hub: convert it again by its route.
    Conversion,
    /// The payment into the withdrawal account did not go through: ask for
    /// it again, under the next attempt's id.
    Payment,
    /// The CASH or PAS is on the withdrawal account on People: move it to
    /// Asset Hub again.
    Withdrawal {
        /// CASH the account received, in payment balance units.
        paid: u128,
    },
    /// The CASH is on People: credit it from `progress`.
    Credit {
        /// What the top-ups claimed, and the attempt to go on with.
        progress: CreditProgress,
    },
}

/// Top-ups claiming a session's CASH, as getcash tracks its claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct CreditProgress {
    /// CASH the earlier attempts claimed, in payment balance units.
    pub credited: u128,
    /// Which attempt is running or next, from 0.
    pub attempt: u8,
    /// The attempt's claim, `None` until it is sized from what the account
    /// holds.
    pub claim: Option<Claim>,
    /// When crediting started, or was last retried, in Unix milliseconds.
    pub started_ms: u64,
}

/// One top-up, kept from when it is sized so that registering it again
/// after a restart asks for the same amount under the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct Claim {
    /// CASH it claims, in payment balance units.
    pub amount: u128,
    /// When it was sized, then when the host accepted it, in Unix
    /// milliseconds.
    pub since_ms: u64,
    /// Whether the host accepted it.
    pub registered: bool,
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
            deposit: None,
            quote: None,
            landed: None,
            withdrawal: None,
            stamps: Vec::new(),
            acknowledged: false,
        }
    }

    /// The step the session is at.
    pub fn step(&self) -> FundingStep {
        match &self.stage {
            FundingStage::Open if self.deposit.is_some() => FundingStep::AwaitingDeposit,
            FundingStage::Open if self.withdrawal.is_some() => FundingStep::AwaitingPayment,
            FundingStage::Paid {
                submission: Some(WithdrawSubmission::Transfer { .. }),
                ..
            } => FundingStep::Converting,
            FundingStage::Paid { .. } => FundingStep::Paid,
            FundingStage::Withdrawn { .. } => FundingStep::Landed,
            FundingStage::Open => FundingStep::Started,
            FundingStage::Converting {
                submission: None, ..
            } => FundingStep::DepositSeen,
            FundingStage::Converting { .. } => FundingStep::Converting,
            FundingStage::Converted => FundingStep::Landed,
            FundingStage::Crediting { .. } => FundingStep::Claiming,
            FundingStage::Delivered { .. } => FundingStep::Settled,
            FundingStage::Failed {
                reason: FundingFailure::Expired,
                ..
            } => FundingStep::Expired,
            FundingStage::Failed {
                reason: FundingFailure::Cancelled,
                ..
            } => FundingStep::Cancelled,
            FundingStage::Failed { .. } => FundingStep::Failed,
        }
    }

    /// Stamp the step in flight the session is at, at `now_ms`, unless it
    /// was reached before: each keeps the time it was first reached, as
    /// getcash stamps its stages. The start and end are the session's own
    /// times, so a retried session that ends again shows its last end.
    /// Returns whether a stamp was added.
    pub fn stamp(&mut self, now_ms: u64) -> bool {
        let step = self.step();
        let in_flight = !matches!(
            step,
            FundingStep::Started
                | FundingStep::Settled
                | FundingStep::Expired
                | FundingStep::Cancelled
                | FundingStep::Failed
        );
        if !in_flight || self.stamps.iter().any(|stamp| stamp.step == step) {
            return false;
        }
        self.stamps.push(FundingStamp { step, at_ms: now_ms });
        true
    }

    /// Whether the session is still going, for the host's lists: not ended,
    /// or failed with its funds where a retry can reach them, which getcash
    /// shows in progress rather than as an outcome.
    pub fn in_flight(&self) -> bool {
        !self.is_terminal() || matches!(self.stage, FundingStage::Failed { resume: Some(_), .. })
    }

    /// Whether the host still has to hear of the session when funding
    /// resumes: it is in flight, or it ended and the host has not recorded
    /// the outcome.
    pub fn needs_handoff(&self) -> bool {
        self.in_flight() || !self.acknowledged
    }

    /// The deposit account as an Asset Hub address, SS58 with prefix 0, the
    /// form getcash shows and providers such as Chainflip require.
    pub fn deposit_address(&self) -> Option<String> {
        self.deposit.as_ref().map(|deposit| asset_hub_address(&deposit.account))
    }

    /// Whether the session has ended.
    pub fn is_terminal(&self) -> bool {
        self.settled_at_ms().is_some()
    }

    /// When the session ended, if it has.
    pub fn settled_at_ms(&self) -> Option<u64> {
        match self.stage {
            FundingStage::Open
            | FundingStage::Converting { .. }
            | FundingStage::Converted
            | FundingStage::Crediting { .. }
            | FundingStage::Paid { .. }
            | FundingStage::Withdrawn { .. } => None,
            FundingStage::Delivered { settled_at_ms, .. } => Some(settled_at_ms),
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
            (
                FundingStage::Converting { .. }
                | FundingStage::Converted
                | FundingStage::Crediting { .. }
                | FundingStage::Paid { .. }
                | FundingStage::Withdrawn { .. },
                _,
            ) => HostFundingStatusSubscribeItem::Converting,
            (FundingStage::Delivered { credited, .. }, _) => {
                HostFundingStatusSubscribeItem::Delivered {
                    credited: *credited,
                }
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
        self.fail_resumable(reason, None, now_ms)
    }

    /// [`Self::fail`], leaving where a retry picks up.
    fn fail_resumable(
        &mut self,
        reason: FundingFailure,
        resume: Option<FundingResume>,
        now_ms: u64,
    ) -> bool {
        if self.is_terminal() {
            return false;
        }
        self.stage = FundingStage::Failed {
            reason,
            settled_at_ms: now_ms,
            resume,
        };
        true
    }

    /// Whether the expiry sweep ends this session at its deadline: an open
    /// one with no deposit or withdrawal account. One with an account is
    /// ended by its watch, after a read that shows its funds did not arrive.
    pub fn expires_by_sweep(&self) -> bool {
        self.stage == FundingStage::Open && self.deposit.is_none() && self.withdrawal.is_none()
    }

    /// Expire the session if the sweep owns it and its deadline passed.
    /// Returns whether it expired.
    pub fn expire_if_due(&mut self, now_ms: u64) -> bool {
        self.expires_by_sweep()
            && now_ms >= self.deadline_ms
            && self.fail(FundingFailure::Expired, now_ms)
    }

    /// The route for a provider delivering `expected` of `asset`, judged
    /// against the quote frozen for that asset: the quoted route when it
    /// covers the quote, the deposit needed when it falls short, `None`
    /// without a quote for the asset.
    pub fn quoted_route(
        &self,
        asset: DepositAsset,
        expected: u128,
    ) -> Option<Result<ConversionRoute, u128>> {
        let quote = self.quote.filter(|quote| quote.asset == asset)?;
        Some(if expected >= quote.deposit {
            Ok(quote.route)
        } else {
            Err(quote.deposit)
        })
    }

    /// The deposit account a session still reads: an open session's, or one
    /// that ended with the deposit recoverable, for [`LATE_WATCH_MS`] after.
    pub fn watched_deposit(&self, now_ms: u64) -> Option<&FundingDeposit> {
        self.deposit
            .as_ref()
            .filter(|_| self.stage == FundingStage::Open || self.recoverable_since(now_ms))
    }

    /// Whether the session ended in a way its deposit can come back from,
    /// within the late watch window: it expired or was cancelled, or its
    /// conversion was refused or held while the funds stayed on the account.
    fn recoverable_since(&self, now_ms: u64) -> bool {
        match &self.stage {
            FundingStage::Failed {
                reason,
                settled_at_ms,
                resume,
            } => {
                let recoverable = matches!(reason, FundingFailure::Expired | FundingFailure::Cancelled)
                    || *resume == Some(FundingResume::Conversion);
                recoverable && now_ms.saturating_sub(*settled_at_ms) <= LATE_WATCH_MS
            }
            _ => false,
        }
    }

    /// Cancel the session while nothing has arrived, as getcash cancels a
    /// request: one still open, or one that expired, and only while its
    /// deposit account, as last read, holds nothing. A cancelled session's
    /// account is still read for the late watch window, so a payment that
    /// arrives after all is converted.
    pub fn cancel(&mut self, now_ms: u64) -> Result<(), CancelRefusal> {
        let cancellable = matches!(
            self.stage,
            FundingStage::Open
                | FundingStage::Failed {
                    reason: FundingFailure::Expired,
                    ..
                }
                | FundingStage::Failed {
                    resume: Some(FundingResume::Payment),
                    ..
                }
        );
        if !cancellable {
            return Err(CancelRefusal::Underway);
        }
        if self.deposit.as_ref().is_some_and(|deposit| !deposit.holdings.is_empty()) {
            return Err(CancelRefusal::FundsArrived);
        }
        if self.withdrawal.as_ref().is_some_and(|withdrawal| withdrawal.taken) {
            return Err(CancelRefusal::Underway);
        }
        self.stage = FundingStage::Failed {
            reason: FundingFailure::Cancelled,
            settled_at_ms: now_ms,
            resume: None,
        };
        // An expired session's outcome changes, so the host hears it again.
        self.acknowledged = false;
        Ok(())
    }

    /// Pick a failed session up where its funds are, as getcash's "try
    /// again" does: a refused or held conversion is tried again by its route
    /// with its refusals cleared, an unclaimed or timed-out credit goes on
    /// from its progress. The route and quote stay as they were.
    pub fn retry(&mut self, now_ms: u64) -> Result<(), RetryRefusal> {
        let FundingStage::Failed {
            resume: Some(resume),
            ..
        } = self.stage
        else {
            return Err(RetryRefusal::NotResumable);
        };
        self.stage = match resume {
            FundingResume::Conversion => {
                let deposited = self
                    .deposit
                    .as_ref()
                    .map_or(0, |deposit| deposit.held(deposit.asset));
                if deposited == 0 {
                    return Err(RetryRefusal::NothingHeld);
                }
                FundingStage::Converting {
                    deposited,
                    refusals: 0,
                    submission: None,
                }
            }
            FundingResume::Payment => {
                // Past the late watch nothing reads the account, so CASH that
                // reached it since would be asked for twice.
                let watched = self.watched_withdrawal(now_ms).is_some();
                let withdrawal = self
                    .withdrawal
                    .as_mut()
                    .filter(|_| watched)
                    .ok_or(RetryRefusal::NotResumable)?;
                // A reused id would be taken for the payment already made.
                withdrawal.attempt = withdrawal.attempt.checked_add(1).ok_or(RetryRefusal::NotResumable)?;
                withdrawal.since_ms = now_ms;
                withdrawal.taken = false;
                FundingStage::Open
            }
            FundingResume::Withdrawal { paid } => FundingStage::Paid {
                paid,
                rejections: 0,
                refused_since_ms: None,
                submission: None,
            },
            FundingResume::Credit { progress } => FundingStage::Crediting {
                progress: CreditProgress {
                    claim: progress.claim.map(|claim| Claim {
                        since_ms: now_ms,
                        ..claim
                    }),
                    started_ms: now_ms,
                    ..progress
                },
            },
        };
        Ok(())
    }

    /// Balance of the deposit's asset at which it counts as delivered, as
    /// getcash gates a payment: the deposit quoted for that asset, else what
    /// the provider was asked for.
    pub fn deposit_gate(&self) -> Option<u128> {
        let deposit = self.deposit.as_ref()?;
        let quoted = self.quote.filter(|quote| quote.asset == deposit.asset);
        Some(quoted.map_or(deposit.expected, |quote| quote.deposit))
    }

    /// What arrived on the deposit account that does not match the request.
    pub fn deposit_mismatch(&self) -> Option<DepositMismatch> {
        self.deposit.as_ref()?.mismatch_against(self.deposit_gate()?)
    }

    /// Record a finalized reading of what the deposit account holds, taken at
    /// `now_ms`. An open session converts once its asset reaches the gate and
    /// expires if it has not by the deadline; one that expired converts too
    /// when its deposit arrives within the late watch window. Returns whether
    /// the host should hear of it: a stage change, or new holdings on a
    /// session still in flight.
    pub fn observe_holdings(&mut self, holdings: Vec<DepositHolding>, now_ms: u64) -> bool {
        let Some(deposit) = self.deposit.as_mut() else {
            return false;
        };
        let recorded = deposit.holdings != holdings;
        deposit.holdings = holdings;
        let held = deposit.held(deposit.asset);
        let Some(gate) = self.deposit_gate() else {
            return false;
        };
        let arrived_late = matches!(
            self.stage,
            FundingStage::Failed {
                reason: FundingFailure::Expired | FundingFailure::Cancelled,
                ..
            }
        ) && self.recoverable_since(now_ms)
            && held >= gate;
        if arrived_late {
            self.stage = FundingStage::Open;
        }
        if self.awaited_deposit().is_none() {
            // An ended session still watched is announced too, so a short or
            // stray payment after a cancel or expiry can be accepted.
            return recorded && (!self.is_terminal() || self.recoverable_since(now_ms));
        }
        if held >= gate {
            self.stage = FundingStage::Converting {
                deposited: held,
                refusals: 0,
                submission: None,
            };
            return true;
        }
        let expired = now_ms >= self.deadline_ms && self.fail(FundingFailure::Expired, now_ms);
        expired || recorded
    }

    /// Convert what arrived of the mismatched `asset` by `route` instead of
    /// what was asked: getcash's "continue with what arrived". `arrived` is
    /// the balance the route was chosen for, refused if the account has
    /// changed since. An open session waits for it again; one that ended
    /// recoverably reopens for a fresh window.
    pub fn accept_arrival(
        &mut self,
        asset: DepositAsset,
        arrived: u128,
        route: ConversionRoute,
        now_ms: u64,
    ) -> Result<(), AcceptRefusal> {
        let reopens = self.recoverable_since(now_ms);
        if self.stage != FundingStage::Open && !reopens {
            return Err(AcceptRefusal::NotAcceptable);
        }
        let mismatched = match self.deposit_mismatch() {
            Some(DepositMismatch::Short { asset, amount } | DepositMismatch::WrongAsset { asset, amount }) => {
                Some((asset, amount))
            }
            None => None,
        };
        match mismatched {
            None => return Err(AcceptRefusal::NothingArrived),
            Some((mismatched, _)) if mismatched != asset => return Err(AcceptRefusal::NotMismatched),
            Some((_, amount)) if amount != arrived => return Err(AcceptRefusal::Changed),
            Some(_) => {}
        }
        let deposit = self.deposit.as_mut().ok_or(AcceptRefusal::NotAcceptable)?;
        deposit.asset = asset;
        deposit.expected = arrived;
        deposit.route = route;
        deposit.target = None;
        // The quoted terms are for what was asked, not for what arrived.
        self.quote = None;
        if reopens {
            self.stage = FundingStage::Open;
            self.deadline_ms = now_ms.saturating_add(SESSION_WINDOW_MS);
        }
        Ok(())
    }

    /// Give an open outbound session that names its amount its withdrawal
    /// account. Returns whether it was given one.
    pub fn assign_withdrawal(&mut self, withdrawal: FundingWithdrawal) -> bool {
        let assignable = self.awaits_withdrawal();
        if assignable {
            self.withdrawal = Some(withdrawal);
        }
        assignable
    }

    /// Whether the session is an open outbound one that names its amount and
    /// has no withdrawal account yet.
    pub fn awaits_withdrawal(&self) -> bool {
        self.direction == FundingDirection::Out
            && self.stage == FundingStage::Open
            && self.amount.is_some()
            && self.withdrawal.is_none()
    }

    /// The withdrawal account a session still reads: an open session's, or
    /// one that expired, was cancelled or had its payment fail, for
    /// [`LATE_WATCH_MS`] after, since CASH that reaches the account is the
    /// user's, as getcash completes a payment its key shows from any state
    /// before it moves on.
    pub fn watched_withdrawal(&self, now_ms: u64) -> Option<&FundingWithdrawal> {
        let recent = self
            .settled_at_ms()
            .is_none_or(|settled_at_ms| now_ms.saturating_sub(settled_at_ms) <= LATE_WATCH_MS);
        self.withdrawal.as_ref().filter(|_| self.payment_pending() && recent)
    }

    /// Whether a withdrawal's payment can still arrive and move the session
    /// on: it is open, or it expired, was cancelled or had its payment fail.
    pub fn payment_pending(&self) -> bool {
        self.withdrawal.is_some()
            && match &self.stage {
                FundingStage::Open => true,
                FundingStage::Failed { reason, resume, .. } => {
                    matches!(reason, FundingFailure::Expired | FundingFailure::Cancelled)
                        || *resume == Some(FundingResume::Payment)
                }
                _ => false,
            }
    }

    /// Record a finalized reading of the withdrawal account's CASH on
    /// People and the host's word on the payment, taken at `now_ms`, as
    /// getcash judges a withdrawal's payment. CASH on the account means it
    /// was paid, whatever the host says: the session waits for it even once
    /// the host reports the payment complete. Otherwise the host's latest
    /// word for the current attempt decides: under way or done, it no longer
    /// expires; failed, the session ends for a retry; unknown to the host,
    /// it was never taken, and one never taken expires after its window.
    /// Returns whether the session changed.
    pub fn observe_withdrawal(&mut self, cash: u128, payment: PaymentReading, now_ms: u64) -> bool {
        if !self.payment_pending() {
            return false;
        }
        if cash > 0 {
            self.stage = FundingStage::Paid {
                paid: cash,
                rejections: 0,
                refused_since_ms: None,
                submission: None,
            };
            return true;
        }
        if self.stage != FundingStage::Open {
            return false;
        }
        let Some(withdrawal) = self.withdrawal.as_mut() else {
            return false;
        };
        let word = match payment {
            PaymentReading { attempt, word } if attempt == withdrawal.attempt => word,
            // The host's word on an earlier attempt says nothing of this one.
            PaymentReading { .. } => PaymentWord::Unanswered,
        };
        let taken = match word {
            PaymentWord::Said(HostPaymentStatusSubscribeItem::Failed { reason }) => {
                return self.fail_resumable(
                    FundingFailure::Other {
                        code: PAYMENT_FAILED.into(),
                        message: reason,
                    },
                    Some(FundingResume::Payment),
                    now_ms,
                );
            }
            PaymentWord::Said(_) => true,
            PaymentWord::NotFound => false,
            PaymentWord::Unanswered => withdrawal.taken,
        };
        let changed = taken != withdrawal.taken;
        withdrawal.taken = taken;
        let expired = !taken
            && now_ms.saturating_sub(withdrawal.since_ms) > PAYMENT_WINDOW_MS
            && self.fail(FundingFailure::Expired, now_ms);
        changed || expired
    }

    /// End an open outbound session whose payment the host or user refused,
    /// for a retry under the next attempt's id. Returns whether it changed.
    pub fn refuse_payment(&mut self, message: String, now_ms: u64) -> bool {
        if self.stage != FundingStage::Open || self.withdrawal.is_none() {
            return false;
        }
        self.fail_resumable(
            FundingFailure::Other {
                code: PAYMENT_FAILED.into(),
                message,
            },
            Some(FundingResume::Payment),
            now_ms,
        )
    }

    /// The deposit an open inbound session is waiting on, if one is assigned.
    pub fn awaited_deposit(&self) -> Option<&FundingDeposit> {
        (self.stage == FundingStage::Open)
            .then_some(self.deposit.as_ref())
            .flatten()
    }

    /// The deposit of a session being converted, with its submission so far.
    pub fn converting(&self) -> Option<(&FundingDeposit, Option<ConversionSubmission>)> {
        match (&self.stage, &self.deposit) {
            (FundingStage::Converting { submission, .. }, Some(deposit)) => {
                Some((deposit, *submission))
            }
            _ => None,
        }
    }

    /// Advance a converting session by one step of its conversion. Returns
    /// whether the session changed.
    pub fn advance_conversion(&mut self, step: ConversionStep, now_ms: u64) -> bool {
        let FundingStage::Converting {
            refusals,
            submission,
            ..
        } = &mut self.stage
        else {
            return false;
        };
        match step {
            ConversionStep::Submitted(submitted) => *submission = Some(submitted),
            ConversionStep::Dropped => *submission = None,
            ConversionStep::Refused { reason, psm } => {
                *submission = None;
                if psm != Some(PsmRefusal::WillNotServe) {
                    *refusals = refusals.saturating_add(1);
                    if *refusals < MAX_CONVERSION_REFUSALS {
                        return true;
                    }
                }
                let (code, message) = match psm {
                    Some(PsmRefusal::WillNotServe) => (
                        CONVERSION_HELD,
                        format!("the PSM will not mint this deposit as quoted: {reason}"),
                    ),
                    Some(PsmRefusal::Unavailable) => (
                        CONVERSION_HELD,
                        format!("the PSM refused the mint {MAX_CONVERSION_REFUSALS} times, last: {reason}"),
                    ),
                    None => (CONVERSION_REFUSED, reason),
                };
                return self.fail_resumable(
                    FundingFailure::Other {
                        code: code.into(),
                        message,
                    },
                    Some(FundingResume::Conversion),
                    now_ms,
                );
            }
            ConversionStep::Landed { landed } => {
                self.stage = FundingStage::Converted;
                self.landed = Some(landed);
            }
            ConversionStep::Stalled => {
                return self.fail(
                    FundingFailure::Other {
                        code: "conversion_stalled".into(),
                        message: "the conversion left Asset Hub but never reached People".into(),
                    },
                    now_ms,
                );
            }
        }
        true
    }
}

impl FundingSession {
    /// The deposit of a session awaiting or being credited, with what its
    /// top-ups have claimed so far, `None` before the first.
    pub fn crediting(&self) -> Option<(&FundingDeposit, Option<CreditProgress>)> {
        let deposit = self.deposit.as_ref()?;
        match self.stage {
            FundingStage::Converted => Some((deposit, None)),
            FundingStage::Crediting { progress } => Some((deposit, Some(progress))),
            _ => None,
        }
    }

    /// Advance a session being credited by one step, as getcash settles its
    /// claim. Returns whether it changed.
    pub fn advance_credit(&mut self, step: CreditStep, now_ms: u64) -> bool {
        let progress = match self.stage {
            FundingStage::Converted => CreditProgress {
                credited: 0,
                attempt: 0,
                claim: None,
                started_ms: now_ms,
            },
            FundingStage::Crediting { progress } => progress,
            _ => return false,
        };
        let credited_with = |claimed: u128| progress.credited.saturating_add(claimed);
        match step {
            CreditStep::Sized { amount } => {
                self.stage = FundingStage::Crediting {
                    progress: CreditProgress {
                        claim: Some(Claim {
                            amount,
                            since_ms: now_ms,
                            registered: false,
                        }),
                        ..progress
                    },
                };
                true
            }
            CreditStep::Registered => {
                let Some(claim) = progress.claim.filter(|claim| !claim.registered) else {
                    return false;
                };
                self.stage = FundingStage::Crediting {
                    progress: CreditProgress {
                        claim: Some(Claim {
                            since_ms: now_ms,
                            registered: true,
                            ..claim
                        }),
                        ..progress
                    },
                };
                true
            }
            CreditStep::Claimed { claimed } => {
                self.stage = FundingStage::Delivered {
                    credited: credited_with(claimed),
                    settled_at_ms: now_ms,
                };
                true
            }
            CreditStep::Short { claimed } => {
                let progress = CreditProgress {
                    credited: credited_with(claimed),
                    attempt: progress.attempt.saturating_add(1),
                    claim: None,
                    ..progress
                };
                if progress.attempt < MAX_CLAIM_ATTEMPTS {
                    self.stage = FundingStage::Crediting { progress };
                    return true;
                }
                self.settle_credit(progress, now_ms)
            }
            CreditStep::Drained => self.settle_credit(progress, now_ms),
            // Between attempts nothing is in flight, so what earlier ones
            // claimed is delivered rather than the session failing.
            CreditStep::TimedOut if progress.claim.is_none() && progress.credited > 0 => {
                self.settle_credit(progress, now_ms)
            }
            CreditStep::TimedOut => self.fail_resumable(
                FundingFailure::Other {
                    code: "credit_timeout".into(),
                    message: "the top-up did not finish in time".into(),
                },
                Some(FundingResume::Credit { progress }),
                now_ms,
            ),
            CreditStep::Refused { reason } => self.fail_resumable(
                FundingFailure::Other {
                    code: "credit_refused".into(),
                    message: reason,
                },
                Some(FundingResume::Credit {
                    progress: CreditProgress {
                        claim: None,
                        ..progress
                    },
                }),
                now_ms,
            ),
        }
    }

    /// End crediting with what the top-ups claimed: delivered, partly if
    /// they fell short, or failed with the CASH still on People, to be
    /// retried from `progress`.
    fn settle_credit(&mut self, progress: CreditProgress, now_ms: u64) -> bool {
        if progress.credited > 0 {
            self.stage = FundingStage::Delivered {
                credited: progress.credited,
                settled_at_ms: now_ms,
            };
            return true;
        }
        self.fail_resumable(
            FundingFailure::Other {
                code: CREDIT_UNCLAIMED.into(),
                message: "the host claimed no CASH from the deposit account".into(),
            },
            Some(FundingResume::Credit { progress }),
            now_ms,
        )
    }
}

/// What one pass of crediting found or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditStep {
    /// The next top-up was sized from what the account holds, to be kept
    /// before it is registered.
    Sized {
        /// CASH it claims, in payment balance units.
        amount: u128,
    },
    /// The host accepted the sized top-up.
    Registered,
    /// The running top-up claimed all it asked for, and it is final.
    Claimed {
        /// Amount it claimed, in payment balance units.
        claimed: u128,
    },
    /// The running top-up claimed less than it asked for, or nothing; the
    /// next attempt claims what is left.
    Short {
        /// Amount it claimed, in payment balance units.
        claimed: u128,
    },
    /// Less is left on the account than a top-up can claim.
    Drained,
    /// The running top-up, or crediting as a whole, took too long.
    TimedOut,
    /// The host will not take the deposit account as a top-up source.
    Refused {
        /// Why.
        reason: String,
    },
}

/// How the PSM refused a mint, as getcash classes its dispatch errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsmRefusal {
    /// Minting is stopped or the PSM is at its debt ceiling, which may
    /// pass: counted, the third holds the funds.
    Unavailable,
    /// The fee is above the quote's or the amount is outside what the PSM
    /// takes, which retrying the same mint cannot cure: holds at once.
    WillNotServe,
}

impl PsmRefusal {
    /// The class of the PSM pallet error named `error`, `None` for one that
    /// is not a refusal.
    pub fn of(error: &str) -> Option<Self> {
        match error {
            "MintingStopped" | "AllSwapsStopped" | "ExceedsMaxPsmDebt" => Some(Self::Unavailable),
            "FeeTooHigh" | "BelowMinimumSwap" | "AmountTooSmallAfterConversion" => {
                Some(Self::WillNotServe)
            }
            _ => None,
        }
    }
}

/// A withdrawal transaction on its way on People.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum WithdrawSubmission {
    /// The pool swap buying the PAS the XCM's fees need.
    Swap {
        /// The account's nonce it was signed with.
        nonce: u32,
        /// Last People block it can be included in.
        valid_until_block: u64,
        /// The account's PAS on People just before, which a swap that went
        /// through adds to.
        pas_before: u128,
    },
    /// The XCM moving everything to Asset Hub.
    Transfer {
        /// The account's nonce it was signed with.
        nonce: u32,
        /// Last People block it can be included in.
        valid_until_block: u64,
        /// When it was submitted, in Unix milliseconds.
        submitted_at_ms: u64,
        /// The landing account's PAS on Asset Hub just before.
        landing_before: u128,
        /// PAS the Asset Hub dry run credited to the landing account.
        expected_landing: u128,
    },
}

/// What one pass of a withdrawal's move to Asset Hub found or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WithdrawStep {
    /// A transaction is about to be submitted.
    Submitted(WithdrawSubmission),
    /// The swap went through; the XCM is next.
    Swapped,
    /// The submitted transaction's era ended unincluded; the next pass
    /// submits again.
    Dropped,
    /// It was included and failed, which costs a fee.
    Rejected {
        /// Why.
        reason: String,
    },
    /// Sizing it, or People's transaction pool, refused it; nothing was
    /// spent, and the next pass tries again.
    Refused {
        /// Why.
        reason: String,
    },
    /// The PAS reached the landing account on Asset Hub.
    Landed {
        /// PAS that landed, in planck.
        landed: u128,
    },
    /// The XCM left People but nothing reached Asset Hub in time; the
    /// assets wait in Asset Hub's trap for the account to claim.
    Stalled,
}

/// Rejections at inclusion, after a passing dry run, before a withdrawal is
/// held, as getcash gives up on a transaction rejected three times.
const MAX_WITHDRAW_REJECTIONS: u8 = 3;
/// How long sizing or the pool may keep refusing a withdrawal's next
/// transaction before it is held, as getcash bounds a run's worked time.
const WITHDRAW_REFUSAL_WINDOW_MS: u64 = 15 * 60 * 1_000;

impl FundingSession {
    /// The withdrawal account and the transaction on its way, of a session
    /// whose CASH is being moved to Asset Hub.
    pub fn withdrawing(&self) -> Option<(&FundingWithdrawal, Option<WithdrawSubmission>)> {
        match (&self.stage, &self.withdrawal) {
            (FundingStage::Paid { submission, .. }, Some(withdrawal)) => Some((withdrawal, *submission)),
            _ => None,
        }
    }

    /// Advance a withdrawal being moved to Asset Hub by one step. Returns
    /// whether the session changed.
    pub fn advance_withdrawal(&mut self, step: WithdrawStep, now_ms: u64) -> bool {
        let FundingStage::Paid {
            paid,
            rejections,
            refused_since_ms,
            submission,
        } = &mut self.stage
        else {
            return false;
        };
        match step {
            WithdrawStep::Submitted(submitted) => {
                *submission = Some(submitted);
                *refused_since_ms = None;
            }
            WithdrawStep::Refused { reason } => {
                *submission = None;
                let since = *refused_since_ms.get_or_insert(now_ms);
                if now_ms.saturating_sub(since) > WITHDRAW_REFUSAL_WINDOW_MS {
                    let paid = *paid;
                    return self.fail_resumable(
                        FundingFailure::Other {
                            code: "withdraw_refused".into(),
                            message: reason,
                        },
                        Some(FundingResume::Withdrawal { paid }),
                        now_ms,
                    );
                }
            }
            WithdrawStep::Swapped | WithdrawStep::Dropped => *submission = None,
            WithdrawStep::Rejected { reason } => {
                *submission = None;
                *rejections = rejections.saturating_add(1);
                if *rejections >= MAX_WITHDRAW_REJECTIONS {
                    let paid = *paid;
                    return self.fail_resumable(
                        FundingFailure::Other {
                            code: "withdraw_rejected".into(),
                            message: format!(
                                "the withdrawal was rejected {MAX_WITHDRAW_REJECTIONS} times, last: {reason}"
                            ),
                        },
                        Some(FundingResume::Withdrawal { paid }),
                        now_ms,
                    );
                }
            }
            WithdrawStep::Landed { landed } => self.stage = FundingStage::Withdrawn { landed },
            WithdrawStep::Stalled => {
                return self.fail(
                    FundingFailure::Other {
                        code: "withdraw_stalled".into(),
                        message: "the withdrawal left People but never reached Asset Hub; its assets wait in Asset Hub's trap for the withdrawal account".into(),
                    },
                    now_ms,
                );
            }
        }
        true
    }
}

/// What one pass of a conversion found or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionStep {
    /// The conversion transaction is about to be submitted.
    Submitted(ConversionSubmission),
    /// The submitted transaction can no longer convert anything: its era
    /// ended unincluded. The next pass submits again.
    Dropped,
    /// A dry run or the transaction pool refused the conversion.
    Refused {
        /// Why, as the chain reported it.
        reason: String,
        /// How the PSM refused it, when it was the PSM.
        psm: Option<PsmRefusal>,
    },
    /// CASH arrived on People.
    Landed {
        /// CASH on People, in payment balance units.
        landed: u128,
    },
    /// The transaction took the deposit on Asset Hub but its CASH never
    /// reached People.
    Stalled,
}

/// Which of a session's accounts under the reserved funding product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum FundingAccountKind {
    /// Where an inbound provider delivers.
    Deposit,
    /// Where a crypto rail returns funds it could not deliver.
    Refund,
    /// Where an outbound session stages funds before paying the provider.
    Withdrawal,
}

/// Why a funding account could not be derived.
#[derive(Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum FundingAccountError {
    /// The label is longer than the 32 bytes entropy derivation takes.
    #[display("funding account label is longer than 32 bytes")]
    LabelTooLong,
    /// The entropy could not be derived.
    #[display("{_0}")]
    Entropy(ProductEntropyError),
    /// The key could not be derived from the entropy.
    #[display("{_0}")]
    Key(ProductAccountError),
}

/// The label of the `number`th account of `kind` for `source_id`:
/// `onramp:eph:<source>:<n>`, `onramp:rf:<source>:<n>` or
/// `wd:eph:<source>:<n>`, the labels getcash uses. `number` counts up from 1
/// per source, so every account can be found again from the seed alone.
pub fn funding_account_label(
    kind: FundingAccountKind,
    source_id: &str,
    number: u32,
) -> Result<String, FundingAccountError> {
    let prefix = match kind {
        FundingAccountKind::Deposit => "onramp:eph",
        FundingAccountKind::Refund => "onramp:rf",
        FundingAccountKind::Withdrawal => "wd:eph",
    };
    let label = format!("{prefix}:{source_id}:{number}");
    (label.len() <= 32)
        .then_some(label)
        .ok_or(FundingAccountError::LabelTooLong)
}

/// The keypair of the `number`th account of `kind` for `source_id`, derived
/// with getcash's scheme: the funding product's `deriveEntropy` for the
/// account's getcash label, taken as a mini secret. getcash derives under its
/// own product id, so its existing burners are other accounts.
pub fn funding_keypair(
    root_entropy: &[u8],
    funding_product_id: &str,
    kind: FundingAccountKind,
    source_id: &str,
    number: u32,
) -> Result<schnorrkel::Keypair, FundingAccountError> {
    let label = funding_account_label(kind, source_id, number)?;
    let entropy = derive_product_entropy(root_entropy, funding_product_id, label.as_bytes())
        .map_err(FundingAccountError::Entropy)?;
    derive_root_keypair_from_entropy(&entropy).map_err(FundingAccountError::Key)
}

/// The mini secret of the same account as [`funding_keypair`], the raw seed
/// a wallet imports it from: what getcash's `burnerSecretOf` hands a user
/// taking funds back by hand.
pub fn funding_mini_secret(
    root_entropy: &[u8],
    funding_product_id: &str,
    kind: FundingAccountKind,
    source_id: &str,
    number: u32,
) -> Result<[u8; 32], FundingAccountError> {
    let label = funding_account_label(kind, source_id, number)?;
    let entropy = derive_product_entropy(root_entropy, funding_product_id, label.as_bytes())
        .map_err(FundingAccountError::Entropy)?;
    substrate_bip39::mini_secret_from_entropy(&entropy, "")
        .map(|mini| mini.to_bytes())
        .map_err(|err| FundingAccountError::Key(ProductAccountError::InvalidEntropy(format!("{err:?}"))))
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

/// The sessions worth keeping: every open one, then the most recently settled
/// ones up to a fixed bound.
///
/// The bound exists because [`CoreStorageKey::FundingSessions`] is one SCALE
/// blob rewritten on every change; the host keeps the full history.
pub fn retained(sessions: impl IntoIterator<Item = FundingSession>) -> Vec<FundingSession> {
    // An ended session whose funds a retry can still reach is kept like an
    // open one, so history cannot push it out.
    let (mut open, mut settled): (Vec<_>, Vec<_>) = sessions.into_iter().partition(FundingSession::in_flight);
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

/// `account` as an Asset Hub address: SS58 with prefix 0.
pub fn asset_hub_address(account: &[u8; 32]) -> String {
    const ASSET_HUB_SS58_PREFIX: u8 = 0;
    let mut bytes = vec![ASSET_HUB_SS58_PREFIX];
    bytes.extend_from_slice(account);
    let checksum = sp_crypto_hashing::blake2_512(&[b"SS58PRE".as_slice(), &bytes].concat());
    bytes.extend_from_slice(&checksum[..2]);
    bs58::encode(bytes).into_string()
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

/// Reserve the next account number for `source_id`, counting up from 1.
///
/// Counters are never reset, so no two sessions on this device share an
/// account. A blob that does not decode is an error rather than a reset for
/// the same reason. Counters are per device: the same seed on a new install
/// starts from 1 again, so whoever hands an account to a provider must first
/// check it is empty on chain.
pub async fn next_account_number(
    storage: &(impl CoreStorage + ?Sized),
    source_id: &str,
) -> Result<u32, FundingSessionError> {
    let storage_error = |reason: String| FundingSessionError::Storage { reason };
    let mut counters = match storage
        .read_core_storage(CoreStorageKey::FundingAccountCounters)
        .await
        .map_err(|err| storage_error(err.reason))?
    {
        Some(blob) => BTreeMap::<String, u32>::decode(&mut blob.as_slice())
            .ok()
            .filter(|counters| counters.encoded_size() == blob.len())
            .ok_or_else(|| storage_error("funding account counters do not decode".into()))?,
        None => BTreeMap::new(),
    };
    let counter = counters.entry(source_id.to_string()).or_default();
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| storage_error(format!("funding accounts for {source_id} exhausted")))?;
    let number = *counter;
    storage
        .write_core_storage(CoreStorageKey::FundingAccountCounters, counters.encode())
        .await
        .map_err(|err| storage_error(err.reason))?;
    Ok(number)
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
                resume: None,
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
    fn open_sessions_come_first_and_settled_history_keeps_the_newest() {
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

    // getcash cancels only while nothing has happened: not once the deposit
    // was seen, not while anything is on the account, which a cancel would
    // strand; an expired request can still be cancelled to hide it.
    #[test]
    fn a_session_is_cancelled_only_while_nothing_arrived() {
        let usdt = DepositAsset::Asset(1984);
        let cancel = |mut session: FundingSession| {
            session.cancel(NOW + 1).map(|()| (session.stage, session.acknowledged))
        };
        let cancelled = FundingStage::Failed {
            reason: FundingFailure::Cancelled,
            settled_at_ms: NOW + 1,
            resume: None,
        };
        let awaiting = FundingSession {
            deposit: Some(with_holdings(&[])),
            ..session(FundingDirection::In)
        };

        assert_eq!(
            [
                cancel(session(FundingDirection::Out)),
                cancel(awaiting.clone()),
                cancel(FundingSession {
                    acknowledged: true,
                    ..expired("fs_1", NOW)
                }),
                cancel(FundingSession {
                    deposit: Some(with_holdings(&[(usdt, 1)])),
                    ..awaiting
                }),
                cancel(converting()),
                cancel(landed()),
            ],
            [
                Ok((cancelled.clone(), false)),
                Ok((cancelled.clone(), false)),
                Ok((cancelled, false)),
                Err(CancelRefusal::FundsArrived),
                Err(CancelRefusal::Underway),
                Err(CancelRefusal::Underway),
            ]
        );
    }

    // A buyer who paid after cancelling is still owed the CASH, so a full
    // deposit on a cancelled session converts within the late watch window,
    // as getcash revives a cancelled request when money is seen.
    #[test]
    fn a_payment_after_a_cancel_is_still_converted() {
        let mut cancelled = FundingSession {
            deposit: Some(with_holdings(&[])),
            ..session(FundingDirection::In)
        };
        cancelled.cancel(NOW).expect("cancelled");
        let paid = vec![DepositHolding {
            asset: DepositAsset::Asset(1984),
            balance: 50,
        }];
        let mut too_late = cancelled.clone();

        assert_eq!(
            (
                cancelled.observe_holdings(paid.clone(), NOW + 1),
                cancelled.stage,
                too_late.observe_holdings(paid, NOW + LATE_WATCH_MS + 1),
            ),
            (
                true,
                FundingStage::Converting {
                    deposited: 50,
                    refusals: 0,
                    submission: None,
                },
                false
            )
        );
    }

    fn withdrawing(taken: bool) -> FundingSession {
        let mut session = FundingSession {
            amount: Some(1_000),
            ..session(FundingDirection::Out)
        };
        session.assign_withdrawal(FundingWithdrawal {
            destination_id: "dot-assethub".into(),
            number: 1,
            account: [4; 32],
            attempt: 0,
            since_ms: NOW,
            taken,
        });
        session
    }

    // getcash judges a withdrawal's payment by the account first: CASH there
    // is paid whatever the host says; then by the host's latest word on the
    // current attempt: under way, it no longer expires; failed, even after
    // it was under way, it is retried; unknown, it was never taken; and one
    // never taken expires after its window. A word on an earlier attempt
    // says nothing of this one.
    #[test]
    fn a_withdrawal_payment_is_judged_as_getcash_judges_it() {
        let said = |status| PaymentReading {
            attempt: 0,
            word: PaymentWord::Said(status),
        };
        let word = |word| PaymentReading { attempt: 0, word };
        let observe = |mut session: FundingSession, cash, payment, now_ms| {
            let changed = session.observe_withdrawal(cash, payment, now_ms);
            (changed, session.step(), session.withdrawal.map(|withdrawal| withdrawal.taken))
        };
        let late = NOW + PAYMENT_WINDOW_MS + 1;
        let failed = || said(HostPaymentStatusSubscribeItem::Failed { reason: "declined".into() });
        let processing = || said(HostPaymentStatusSubscribeItem::Processing);
        let mut expired = withdrawing(false);
        expired.observe_withdrawal(0, word(PaymentWord::Unanswered), late);
        let stale = PaymentReading {
            attempt: 1,
            word: PaymentWord::Said(HostPaymentStatusSubscribeItem::Processing),
        };

        assert_eq!(
            [
                observe(withdrawing(false), 0, word(PaymentWord::Unanswered), NOW + 1),
                observe(withdrawing(false), 0, processing(), NOW + 1),
                observe(withdrawing(false), 0, failed(), NOW + 1),
                observe(withdrawing(true), 0, failed(), NOW + 1),
                observe(withdrawing(true), 0, word(PaymentWord::NotFound), NOW + 1),
                observe(withdrawing(false), 0, word(PaymentWord::Unanswered), late),
                observe(withdrawing(true), 0, processing(), late),
                observe(withdrawing(false), 0, stale, late),
                observe(withdrawing(false), 900, word(PaymentWord::Unanswered), NOW + 1),
                observe(expired, 900, word(PaymentWord::Unanswered), late + 1),
            ],
            [
                (false, FundingStep::AwaitingPayment, Some(false)),
                (true, FundingStep::AwaitingPayment, Some(true)),
                (true, FundingStep::Failed, Some(false)),
                (true, FundingStep::Failed, Some(true)),
                (true, FundingStep::AwaitingPayment, Some(false)),
                (true, FundingStep::Expired, Some(false)),
                (false, FundingStep::AwaitingPayment, Some(true)),
                (true, FundingStep::Expired, Some(false)),
                (true, FundingStep::Paid, Some(false)),
                (true, FundingStep::Paid, Some(false)),
            ]
        );
    }

    // A withdrawal's payment is asked for again under the next attempt's
    // id, but not once the account is no longer read, when CASH that reached
    // it would be asked for twice; one that failed can be cancelled, one
    // under way cannot, and none waits on the 24 h sweep, which would end a
    // payment the host is still taking.
    #[test]
    fn a_withdrawal_retries_under_a_new_id_and_a_taken_one_cannot_be_cancelled() {
        let mut refused = withdrawing(false);
        refused.refuse_payment("declined".into(), NOW);
        let mut too_late = refused.clone();
        let mut cancelled = refused.clone();
        let retried = refused.retry(NOW + 5).map(|()| refused.withdrawal.clone());

        assert_eq!(
            (
                retried.map(|withdrawal| withdrawal.map(|withdrawal| (withdrawal.attempt, withdrawal.since_ms))),
                too_late.retry(NOW + LATE_WATCH_MS + 1),
                cancelled.cancel(NOW + 1),
                withdrawing(true).cancel(NOW),
                withdrawing(false).cancel(NOW),
                withdrawing(false).expires_by_sweep(),
            ),
            (
                Ok(Some((1, NOW + 5))),
                Err(RetryRefusal::NotResumable),
                Ok(()),
                Err(CancelRefusal::Underway),
                Ok(()),
                false
            )
        );
    }

    fn paid() -> FundingSession {
        let mut session = withdrawing(true);
        session.observe_withdrawal(
            1_000,
            PaymentReading {
                attempt: 0,
                word: PaymentWord::Unanswered,
            },
            NOW,
        );
        session
    }

    // getcash gives a withdrawal up after a transaction is rejected three
    // times at inclusion, with the funds still on the account for a retry,
    // which starts the count again; landing on Asset Hub ends the move.
    #[test]
    fn a_withdrawal_move_is_held_after_three_rejections_and_lands_on_asset_hub() {
        let rejected = || WithdrawStep::Rejected { reason: "no".into() };
        let mut held = paid();
        for _ in 0..3 {
            held.advance_withdrawal(rejected(), NOW);
        }
        let held_stage = held.stage.clone();
        held.retry(NOW + 1).expect("retried");
        let mut landed = paid();
        landed.advance_withdrawal(WithdrawStep::Landed { landed: 950 }, NOW);

        assert_eq!(
            (held_stage, held.stage, landed.step(), landed.stage),
            (
                FundingStage::Failed {
                    reason: FundingFailure::Other {
                        code: "withdraw_rejected".into(),
                        message: "the withdrawal was rejected 3 times, last: no".into(),
                    },
                    settled_at_ms: NOW,
                    resume: Some(FundingResume::Withdrawal { paid: 1_000 }),
                },
                FundingStage::Paid {
                    paid: 1_000,
                    rejections: 0,
                    refused_since_ms: None,
                    submission: None,
                },
                FundingStep::Landed,
                FundingStage::Withdrawn { landed: 950 },
            )
        );
    }

    // A refusal costs nothing and may pass, so it is tried again, as getcash
    // retries a tick that throws, until refusing has gone on longer than a
    // run may take; a transaction that goes out clears it.
    #[test]
    fn refusals_hold_a_withdrawal_only_after_the_window() {
        let refused = || WithdrawStep::Refused { reason: "no quote".into() };
        let mut kept = paid();
        kept.advance_withdrawal(refused(), NOW);
        kept.advance_withdrawal(refused(), NOW + WITHDRAW_REFUSAL_WINDOW_MS);
        let mut held = kept.clone();
        held.advance_withdrawal(refused(), NOW + WITHDRAW_REFUSAL_WINDOW_MS + 1);
        let mut cleared = kept.clone();
        cleared.advance_withdrawal(
            WithdrawStep::Submitted(WithdrawSubmission::Swap {
                nonce: 1,
                valid_until_block: 10,
                pas_before: 0,
            }),
            NOW + 1,
        );

        assert_eq!(
            (kept.step(), held.step(), cleared.stage),
            (
                FundingStep::Paid,
                FundingStep::Failed,
                FundingStage::Paid {
                    paid: 1_000,
                    rejections: 0,
                    refused_since_ms: None,
                    submission: Some(WithdrawSubmission::Swap {
                        nonce: 1,
                        valid_until_block: 10,
                        pas_before: 0,
                    }),
                },
            )
        );
    }

    // A deposit screen and a provider must see the address Asset Hub
    // wallets and Chainflip accept: prefix 0, as getcash encodes it.
    #[test]
    fn a_deposit_address_is_an_asset_hub_address() {
        let alice = hex::decode("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d")
            .expect("hex")
            .try_into()
            .expect("32 bytes");

        assert_eq!(
            asset_hub_address(&alice),
            "15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5"
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

    // A reused number hands a second session an account that may still hold
    // the first one's funds, so counters only ever move up, per source, and a
    // counter blob that cannot be read stops funding instead of restarting.
    #[test]
    fn account_numbers_count_up_per_source_and_never_restart() {
        let storage = stub_platform();
        let next = |source: &str| block_on(next_account_number(storage.as_ref(), source));
        let issued = [next("usdt"), next("usdt"), next("btc"), next("usdt")];
        block_on(storage.write_core_storage(CoreStorageKey::FundingAccountCounters, vec![0xff]))
            .expect("written");

        assert_eq!(
            (issued, next("usdt").is_err()),
            ([Ok(1), Ok(2), Ok(1), Ok(3)], true)
        );
    }

    const SUBMISSION: ConversionSubmission = ConversionSubmission {
        nonce: 4,
        submitted_at_ms: NOW,
        valid_until_block: 164,
        people_before: 0,
        landing: 40,
        spent: 50,
    };

    fn converting() -> FundingSession {
        FundingSession {
            stage: FundingStage::Converting {
                deposited: 50,
                refusals: 0,
                submission: None,
            },
            deposit: Some(FundingDeposit {
                source_id: "usdt-assethub".to_string(),
                number: 1,
                asset: DepositAsset::Asset(1984),
                account: [1; 32],
                expected: 50,
                route: ConversionRoute::Teleport,
                target: None,
                holdings: Vec::new(),
            }),
            ..session(FundingDirection::In)
        }
    }

    fn refuse(session: &mut FundingSession, psm: Option<PsmRefusal>) -> FundingStage {
        session.advance_conversion(
            ConversionStep::Refused {
                reason: "no pool".into(),
                psm,
            },
            NOW,
        );
        session.stage.clone()
    }

    // A dry run that keeps refusing will not start passing, so the third
    // refusal ends the session rather than retrying forever, with the
    // deposit left where a retry can convert it; a refusal clears any
    // submission so the next attempt starts clean.
    #[test]
    fn a_conversion_ends_on_its_third_refusal() {
        let mut session = converting();
        session.advance_conversion(ConversionStep::Submitted(SUBMISSION), NOW);

        assert_eq!(
            [refuse(&mut session, None), refuse(&mut session, None), refuse(&mut session, None)],
            [
                FundingStage::Converting {
                    deposited: 50,
                    refusals: 1,
                    submission: None,
                },
                FundingStage::Converting {
                    deposited: 50,
                    refusals: 2,
                    submission: None,
                },
                FundingStage::Failed {
                    reason: FundingFailure::Other {
                        code: "conversion_refused".into(),
                        message: "no pool".into(),
                    },
                    settled_at_ms: NOW,
                    resume: Some(FundingResume::Conversion),
                },
            ]
        );
    }

    // getcash holds the funds on the first refusal the PSM will never get
    // past, such as a fee above the quote, and on the third it may get past;
    // both leave the deposit for a retry, which starts the count again.
    #[test]
    fn the_psm_holds_the_deposit_as_getcash_holds_it() {
        let held = |message: &str| FundingStage::Failed {
            reason: FundingFailure::Other {
                code: "conversion_held".into(),
                message: message.into(),
            },
            settled_at_ms: NOW,
            resume: Some(FundingResume::Conversion),
        };
        let mut will_not = converting();
        let mut unavailable = converting();
        let unavailable_stages = [
            refuse(&mut unavailable, Some(PsmRefusal::Unavailable)),
            refuse(&mut unavailable, Some(PsmRefusal::Unavailable)),
            refuse(&mut unavailable, Some(PsmRefusal::Unavailable)),
        ];
        let held_deposit = FundingDeposit {
            holdings: vec![DepositHolding {
                asset: DepositAsset::Asset(1984),
                balance: 50,
            }],
            ..converting().deposit.expect("deposit")
        };
        unavailable.deposit = Some(held_deposit);
        let retried = (unavailable.retry(NOW + 1), unavailable.stage.clone());

        assert_eq!(
            (refuse(&mut will_not, Some(PsmRefusal::WillNotServe)), unavailable_stages, retried),
            (
                held("the PSM will not mint this deposit as quoted: no pool"),
                [
                    FundingStage::Converting {
                        deposited: 50,
                        refusals: 1,
                        submission: None,
                    },
                    FundingStage::Converting {
                        deposited: 50,
                        refusals: 2,
                        submission: None,
                    },
                    held("the PSM refused the mint 3 times, last: no pool"),
                ],
                (
                    Ok(()),
                    FundingStage::Converting {
                        deposited: 50,
                        refusals: 0,
                        submission: None,
                    }
                ),
            )
        );
    }

    // The provider is told the quoted figure, so the quote it was given,
    // not one re-priced when the account is assigned, decides whether its
    // deposit is enough.
    #[test]
    fn a_deposit_is_judged_against_the_quote_for_its_asset() {
        let usdt = DepositAsset::Asset(1984);
        let session = FundingSession {
            quote: Some(DepositQuote {
                asset: usdt,
                route: ConversionRoute::Psm { fee_ppm: 5_000 },
                deposit: 2_136_987,
            }),
            ..session(FundingDirection::In)
        };

        assert_eq!(
            [
                session.quoted_route(usdt, 2_136_987),
                session.quoted_route(usdt, 2_136_986),
                session.quoted_route(DepositAsset::Asset(1337), 9_000_000),
            ],
            [Some(Ok(ConversionRoute::Psm { fee_ppm: 5_000 })), Some(Err(2_136_987)), None]
        );
    }

    fn with_holdings(holdings: &[(DepositAsset, u128)]) -> FundingDeposit {
        FundingDeposit {
            source_id: "usdt-assethub".into(),
            number: 1,
            asset: DepositAsset::Asset(1984),
            account: [1; 32],
            expected: 50,
            route: ConversionRoute::Psm { fee_ppm: 5_000 },
            target: Some(40),
            holdings: holdings
                .iter()
                .map(|(asset, balance)| DepositHolding {
                    asset: *asset,
                    balance: *balance,
                })
                .collect(),
        }
    }

    // getcash's rules: once the requested asset covers the deposit nothing
    // is wrong; otherwise another asset that arrived comes first, the native
    // token only when nothing else did, since a little of it sent to pay
    // fees must not stand in for the stablecoin; then a short amount.
    #[test]
    fn a_mismatch_is_judged_as_getcash_judges_it() {
        let usdt = DepositAsset::Asset(1984);
        let usdc = DepositAsset::Asset(1337);
        let native = DepositAsset::Native;

        assert_eq!(
            [
                with_holdings(&[(usdt, 50), (usdc, 9)]).mismatch_against(50),
                with_holdings(&[(usdt, 10), (usdc, 9), (native, 3)]).mismatch_against(50),
                with_holdings(&[(usdt, 10), (native, 3)]).mismatch_against(50),
                with_holdings(&[(usdt, 10)]).mismatch_against(50),
                with_holdings(&[]).mismatch_against(50),
            ],
            [
                None,
                Some(DepositMismatch::WrongAsset {
                    asset: usdc,
                    amount: 9
                }),
                Some(DepositMismatch::WrongAsset {
                    asset: native,
                    amount: 3
                }),
                Some(DepositMismatch::Short {
                    asset: usdt,
                    amount: 10
                }),
                None,
            ]
        );
    }

    // Accepting converts what is there instead of what was asked; an expired
    // or refused session reopens for a fresh window while its funds are
    // still watched, and stays ended after.
    #[test]
    fn accepting_what_arrived_reroutes_and_reopens_within_the_watch_window() {
        let usdc = DepositAsset::Asset(1337);
        let open = FundingSession {
            deposit: Some(with_holdings(&[(DepositAsset::Asset(1984), 10), (usdc, 9)])),
            ..session(FundingDirection::In)
        };
        let expired = |settled_at_ms| FundingSession {
            stage: FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms,
                resume: None,
            },
            ..open.clone()
        };
        let delivered = FundingSession {
            deposit: Some(with_holdings(&[(DepositAsset::Asset(1984), 50)])),
            ..session(FundingDirection::In)
        };
        let accept = |mut session: FundingSession, asset, arrived, now_ms| {
            let accepted = session.accept_arrival(asset, arrived, ConversionRoute::Pool, now_ms);
            (accepted, session.stage.clone(), session.deposit.map(|deposit| (deposit.asset, deposit.expected, deposit.target)))
        };
        let late = NOW + LATE_WATCH_MS;

        assert_eq!(
            [
                accept(open.clone(), usdc, 9, NOW).0,
                accept(open.clone(), DepositAsset::Asset(1984), 10, NOW).0,
                accept(open.clone(), usdc, 8, NOW).0,
                accept(delivered, DepositAsset::Asset(1984), 50, NOW).0,
                accept(expired(NOW), usdc, 9, late).0,
                accept(expired(NOW), usdc, 9, late + 1).0,
            ],
            [
                Ok(()),
                Err(AcceptRefusal::NotMismatched),
                Err(AcceptRefusal::Changed),
                Err(AcceptRefusal::NothingArrived),
                Ok(()),
                Err(AcceptRefusal::NotAcceptable),
            ]
        );
        assert_eq!(
            accept(expired(NOW), usdc, 9, late),
            (Ok(()), FundingStage::Open, Some((usdc, 9, None)))
        );
    }

    // getcash gates a payment on its quoted deposit, so a provider asked for
    // more than the quote has delivered once the quote is covered; a full
    // deposit that arrives after the session expired still converts within
    // the late watch window, with no one having to accept it, and a short one
    // is announced so it can be accepted.
    #[test]
    fn a_deposit_converts_at_its_quote_and_also_when_it_arrives_late() {
        let usdt = DepositAsset::Asset(1984);
        let holding = |balance| vec![DepositHolding { asset: usdt, balance }];
        let quoted = FundingSession {
            deposit: Some(with_holdings(&[])),
            quote: Some(DepositQuote {
                asset: usdt,
                route: ConversionRoute::Psm { fee_ppm: 5_000 },
                deposit: 45,
            }),
            ..session(FundingDirection::In)
        };
        let expired = FundingSession {
            stage: FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms: NOW,
                resume: None,
            },
            ..quoted.clone()
        };
        let observe = |mut session: FundingSession, balance, now_ms| {
            let heard = session.observe_holdings(holding(balance), now_ms);
            (heard, session.stage)
        };
        let converting = FundingStage::Converting {
            deposited: 45,
            refusals: 0,
            submission: None,
        };

        assert_eq!(
            [
                observe(quoted.clone(), 44, NOW),
                observe(quoted, 45, NOW),
                observe(expired.clone(), 44, NOW + 1),
                observe(expired.clone(), 45, NOW + LATE_WATCH_MS),
                observe(expired.clone(), 45, NOW + LATE_WATCH_MS + 1),
            ],
            [
                (true, FundingStage::Open),
                (true, converting.clone()),
                (true, expired.stage.clone()),
                (true, converting),
                (false, expired.stage),
            ]
        );
    }

    // A user taking funds back by hand imports this seed into a wallet, so it
    // must be what getcash's `burnerSecretOf` hands out: `entropyToMiniSecret`
    // of the label's entropy (the vector is from that library), and the
    // account it opens must be the one the core signs with.
    #[test]
    fn the_exported_seed_is_getcashs_and_opens_the_same_account() {
        let root = [9u8; 32];
        let seed = funding_mini_secret(&root, "fund.dot", FundingAccountKind::Deposit, "usdt-assethub", 1)
            .expect("seed");
        let opened = schnorrkel::MiniSecretKey::from_bytes(&seed)
            .expect("mini secret")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
            .public;

        assert_eq!(
            (
                substrate_bip39::mini_secret_from_entropy(&[7; 32], "")
                    .map(|mini| hex::encode(mini.to_bytes()))
                    .ok(),
                Some(opened),
            ),
            (
                Some("12c532afaa1c0ffe4d0eae23c7038d9e866e4335a00eaa3f0dbb01661295325d".to_string()),
                funding_keypair(&root, "fund.dot", FundingAccountKind::Deposit, "usdt-assethub", 1)
                    .ok()
                    .map(|keypair| keypair.public),
            )
        );
    }

    fn landed() -> FundingSession {
        let mut session = converting();
        session.advance_conversion(ConversionStep::Landed { landed: 49 }, NOW);
        session
    }

    fn credit(session: &mut FundingSession, steps: impl IntoIterator<Item = CreditStep>) -> FundingStage {
        for step in steps {
            session.advance_credit(step, NOW);
        }
        session.stage.clone()
    }

    // Delivered is the one inbound success: it ends the session for
    // subscribers and history, and the credited amount is what they see.
    #[test]
    fn a_credited_session_is_delivered() {
        let mut session = landed();
        session.advance_credit(CreditStep::Sized { amount: 40 }, NOW);
        session.advance_credit(CreditStep::Registered, NOW);
        let crediting = session.crediting().map(|(_, progress)| progress);
        session.advance_credit(CreditStep::Claimed { claimed: 40 }, NOW + 1);

        assert_eq!(
            (crediting, session.wire_item(), session.settled_at_ms()),
            (
                Some(Some(CreditProgress {
                    credited: 0,
                    attempt: 0,
                    claim: Some(Claim {
                        amount: 40,
                        since_ms: NOW,
                        registered: true,
                    }),
                    started_ms: NOW,
                })),
                HostFundingStatusSubscribeItem::Delivered { credited: 40 },
                Some(NOW + 1),
            )
        );
    }

    // getcash adds up what each claim took: a short claim leaves the rest
    // for the next attempt, a final one delivers the total, and after the
    // last attempt whatever was claimed is delivered, short as it is.
    #[test]
    fn short_claims_add_up_and_the_last_attempt_settles_for_them() {
        let short = |claimed| CreditStep::Short { claimed };

        assert_eq!(
            [
                credit(&mut landed(), [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short(10)]),
                credit(&mut landed(), [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short(10), CreditStep::Sized { amount: 30 }, CreditStep::Registered, CreditStep::Claimed { claimed: 30 }]),
                credit(&mut landed(), [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short(10), CreditStep::Sized { amount: 30 }, CreditStep::Registered, short(0), CreditStep::Sized { amount: 30 }, CreditStep::Registered, short(5)]),
                credit(&mut landed(), [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short(10), CreditStep::Drained]),
            ],
            [
                FundingStage::Crediting {
                    progress: CreditProgress {
                        credited: 10,
                        attempt: 1,
                        claim: None,
                        started_ms: NOW,
                    },
                },
                FundingStage::Delivered {
                    credited: 40,
                    settled_at_ms: NOW,
                },
                FundingStage::Delivered {
                    credited: 15,
                    settled_at_ms: NOW,
                },
                FundingStage::Delivered {
                    credited: 10,
                    settled_at_ms: NOW,
                },
            ]
        );
    }

    // Between attempts nothing is in flight, so running out of time there
    // delivers what was claimed; with a top-up in flight it fails, to be
    // watched again on a retry.
    #[test]
    fn running_out_of_time_between_attempts_delivers_what_was_claimed() {
        let short = CreditStep::Short { claimed: 10 };
        let sized = CreditStep::Sized { amount: 30 };

        assert_eq!(
            [
                credit(&mut landed(), [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short.clone(), CreditStep::TimedOut]),
                credit(&mut landed(), [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short, sized, CreditStep::TimedOut]),
            ],
            [
                FundingStage::Delivered {
                    credited: 10,
                    settled_at_ms: NOW,
                },
                FundingStage::Failed {
                    reason: FundingFailure::Other {
                        code: "credit_timeout".into(),
                        message: "the top-up did not finish in time".into(),
                    },
                    settled_at_ms: NOW,
                    resume: Some(FundingResume::Credit {
                        progress: CreditProgress {
                            credited: 10,
                            attempt: 1,
                            claim: Some(Claim {
                                amount: 30,
                                since_ms: NOW,
                                registered: false,
                            }),
                            started_ms: NOW,
                        },
                    }),
                },
            ]
        );
    }

    // A failed session whose funds a retry can still reach must outlive the
    // history bound, or the host could lose track of where they are.
    #[test]
    fn history_keeps_a_failed_session_that_still_holds_funds() {
        let held = FundingSession {
            intent: "fs_held".into(),
            stage: FundingStage::Failed {
                reason: FundingFailure::Expired,
                settled_at_ms: NOW - 1,
                resume: Some(FundingResume::Conversion),
            },
            ..session(FundingDirection::In)
        };
        let history = (0..SETTLED_HISTORY_LIMIT as u64).map(|n| expired(&format!("fs_{n}"), NOW + n));

        assert!(
            retained(history.chain([held.clone()]))
                .iter()
                .any(|session| session.intent == held.intent)
        );
    }

    // CASH no top-up claimed is still on People, so a failed credit is
    // retried as getcash re-arms it: after an unclaimed last attempt with a
    // fresh attempt and id, after a timeout with the same claim watched
    // again, and after a refused source by registering the attempt again.
    #[test]
    fn a_failed_credit_is_retried_from_where_it_stopped() {
        let short = CreditStep::Short { claimed: 0 };
        let retried = |mut session: FundingSession| {
            let retried = session.retry(NOW + 5);
            (retried, session.stage)
        };
        let progress = |attempt, claim| FundingStage::Crediting {
            progress: CreditProgress {
                credited: 0,
                attempt,
                claim,
                started_ms: NOW + 5,
            },
        };
        let mut unclaimed = landed();
        let unclaimed_stage = credit(
            &mut unclaimed,
            [CreditStep::Sized { amount: 40 }, CreditStep::Registered, short.clone(), CreditStep::Sized { amount: 40 }, CreditStep::Registered, short.clone(), CreditStep::Sized { amount: 40 }, CreditStep::Registered, short],
        );
        let mut timed_out = landed();
        credit(&mut timed_out, [CreditStep::Sized { amount: 40 }, CreditStep::Registered, CreditStep::TimedOut]);
        let mut refused = landed();
        credit(&mut refused, [CreditStep::Refused { reason: "no".into() }]);

        assert_eq!(
            (
                unclaimed_stage,
                [retried(unclaimed), retried(timed_out), retried(refused), retried(landed())],
            ),
            (
                FundingStage::Failed {
                    reason: FundingFailure::Other {
                        code: "credit_unclaimed".into(),
                        message: "the host claimed no CASH from the deposit account".into(),
                    },
                    settled_at_ms: NOW,
                    resume: Some(FundingResume::Credit {
                        progress: CreditProgress {
                            credited: 0,
                            attempt: 3,
                            claim: None,
                            started_ms: NOW,
                        },
                    }),
                },
                [
                    (Ok(()), progress(3, None)),
                    (
                        Ok(()),
                        progress(
                            0,
                            Some(Claim {
                                amount: 40,
                                since_ms: NOW + 5,
                                registered: true,
                            })
                        )
                    ),
                    (Ok(()), progress(0, None)),
                    (Err(RetryRefusal::NotResumable), landed().stage),
                ],
            )
        );
    }

    // CASH on People is what the user is owed, so landing ends conversion
    // whatever the submission state, and the subscriber keeps seeing
    // converting until it is credited.
    #[test]
    fn landing_on_people_ends_the_conversion() {
        let mut session = converting();
        session.advance_conversion(ConversionStep::Submitted(SUBMISSION), NOW);
        let submitted = session.converting().and_then(|(_, submission)| submission);
        session.advance_conversion(ConversionStep::Landed { landed: 49 }, NOW);

        assert_eq!(
            (submitted, session.stage.clone(), session.wire_item(), session.is_terminal()),
            (
                Some(SUBMISSION),
                FundingStage::Converted,
                HostFundingStatusSubscribeItem::Converting,
                false,
            )
        );
    }

    // Funds sit in these accounts, so a label that drifts between releases
    // strands them. The labels are getcash's, byte for byte, unpadded.
    #[test]
    fn funding_account_labels_are_getcash_labels() {
        assert_eq!(
            [
                funding_account_label(FundingAccountKind::Deposit, "usdt-assethub", 1),
                funding_account_label(FundingAccountKind::Refund, "btc", 2),
                funding_account_label(FundingAccountKind::Withdrawal, "dot-assethub", 3),
                funding_account_label(FundingAccountKind::Deposit, "x".repeat(40).as_str(), 1),
            ],
            [
                Ok("onramp:eph:usdt-assethub:1".to_string()),
                Ok("onramp:rf:btc:2".to_string()),
                Ok("wd:eph:dot-assethub:3".to_string()),
                Err(FundingAccountError::LabelTooLong),
            ]
        );
    }

    // getcash turns its `deriveEntropy(label)` into the burner with
    // `entropyToMiniSecret` and `sr25519CreateDerive(mini)("")`. The keys are
    // from those libraries, fed the entropy core's `deriveEntropy` gives
    // `fund.dot` for the label (itself pinned to dotli's vector), so the same
    // root reaches the same account through either implementation.
    #[test]
    fn a_funding_key_is_the_one_getcash_derives_from_the_same_entropy() {
        let key = |root: &[u8]| {
            funding_keypair(root, "fund.dot", FundingAccountKind::Deposit, "usdt-assethub", 1)
                .map(|keypair| hex::encode(keypair.public.to_bytes()))
        };

        assert_eq!(
            (
                derive_root_keypair_from_entropy(&[7; 32])
                    .map(|keypair| hex::encode(keypair.public.to_bytes())),
                key(&[9; 32]),
            ),
            (
                Ok("ae78b88f68f8a3391cd7d1a8908766e1d068b2c1db6244a373e8b643e49d085f".to_string()),
                Ok("fefa1fc85ecec3e8efa2cf47672fe85220dfa74c4aeda155b673f414b141b054".to_string()),
            )
        );
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
