// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::collections::HashMap;
use std::future::Future;
use {parking_lot::Mutex, std::sync::Arc};

use async_trait::async_trait;
use futures::future::join_all;
use rand::RngCore;
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tracing::{error, warn};

use crate::allocator::CoinAllocator;
use crate::clock::Clock;
use crate::denomination::DenominationBreakdownContext;
use crate::keys::CoinKeypairFactory;
use crate::memo::{MemoEntry, TransferMemo};
use crate::model::{Coin, CoinState, EffectivePrivacy, Voucher, VoucherRemoteState};
use crate::repo::{CoinRepository, TransferContext, TransferStateCommitter, VoucherRepository};
use crate::ring_proof::{PersonOriginKind, ResolvedUnloadToken, RingProofParams};
use crate::selection::{
    CoinSelectionError, CoinSelectionResult, CoinSelector, PrivacyLevel, RecyclerKey,
    TransferStrategy,
};
use crate::wal::{
    CheckpointBlock, TransferWalEntry, WalCoinRef, WalOperation, WalPayload, WalStore,
    operation_entry_id,
};

const PREVIEW_LIFETIME_MS: i64 = 2 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPreviewChoice {
    Full,
    NonDegraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPreviewStrategy {
    ExactMatch,
    Split,
    UnloadIntoCoins,
}

/// Secret-free preview fields suitable for an FFI record. `preview_id` is
/// opaque and meaningful only to the session-scoped service that created it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueTransferPreview {
    pub preview_id: String,
    pub session_generation: u64,
    pub full_amount: u128,
    /// Maximum source value that may leave the purse, in raw planks.
    /// Unload bounds conservatively include all selected voucher value.
    pub max_debit_amount: u128,
    pub non_degraded_amount: u128,
    pub is_degraded: bool,
    pub strategy: TransferPreviewStrategy,
    pub expires_at_ms: i64,
}

/// Public-only voucher snapshot for selection diagnostics: exposes selection
/// state without exposing any voucher secret key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoucherSelectionDiagnostic {
    pub derivation_index: u32,
    pub exponent: i16,
    pub local_state: crate::model::VoucherLocalState,
    pub remote_state: crate::model::VoucherRemoteState,
    pub stored_privacy: crate::model::VoucherPrivacyLevel,
    pub effective_privacy: EffectivePrivacy,
    pub ready_at_ms: i64,
}

pub fn voucher_selection_diagnostics(
    vouchers: &[Voucher],
    now_ms: i64,
) -> Vec<VoucherSelectionDiagnostic> {
    vouchers
        .iter()
        .map(|voucher| VoucherSelectionDiagnostic {
            derivation_index: voucher.derivation_index,
            exponent: voucher.exponent,
            local_state: voucher.local_state,
            remote_state: voucher.remote_state,
            stored_privacy: voucher.privacy,
            effective_privacy: voucher.effective_privacy(now_ms),
            ready_at_ms: voucher.ready_at_ms,
        })
        .collect()
}

/// Redacted aggregate emitted when authoritative preview selection fails.
/// Values are represented only by decimal digit counts, so production logs
/// reveal neither exact balances nor any coin/voucher key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorSnapshotDiagnostic {
    pub selectable_coins: usize,
    pub expiring_coins: usize,
    pub locked_coins: usize,
    pub unloadable_vouchers: usize,
    pub locked_vouchers: usize,
    pub full_vouchers: usize,
    pub degraded_vouchers: usize,
    pub selectable_coin_value_digits: usize,
    pub unloadable_voucher_value_digits: usize,
}

pub fn selector_snapshot_diagnostic(
    coins: &[Coin],
    vouchers: &[Voucher],
    context: &DenominationBreakdownContext,
    now_ms: i64,
) -> SelectorSnapshotDiagnostic {
    let selectable_coin_value = coins
        .iter()
        .filter(|coin| coin.is_selectable())
        .fold(0u128, |sum, coin| {
            sum.saturating_add(context.value_in_planks(coin.exponent))
        });
    let unloadable_voucher_value = vouchers
        .iter()
        .filter(|voucher| voucher.is_unloadable())
        .fold(0u128, |sum, voucher| {
            sum.saturating_add(context.value_in_planks(voucher.exponent))
        });
    SelectorSnapshotDiagnostic {
        selectable_coins: coins.iter().filter(|coin| coin.is_selectable()).count(),
        expiring_coins: coins
            .iter()
            .filter(|coin| {
                coin.state == crate::model::CoinState::Available && coin.is_expiring_soon()
            })
            .count(),
        locked_coins: coins.iter().filter(|coin| !coin.is_selectable()).count(),
        unloadable_vouchers: vouchers
            .iter()
            .filter(|voucher| voucher.is_unloadable())
            .count(),
        locked_vouchers: vouchers
            .iter()
            .filter(|voucher| !voucher.is_unloadable())
            .count(),
        full_vouchers: vouchers
            .iter()
            .filter(|voucher| voucher.effective_privacy(now_ms) == EffectivePrivacy::Full)
            .count(),
        degraded_vouchers: vouchers
            .iter()
            .filter(|voucher| voucher.effective_privacy(now_ms) == EffectivePrivacy::Degraded)
            .count(),
        selectable_coin_value_digits: decimal_digits(selectable_coin_value),
        unloadable_voucher_value_digits: decimal_digits(unloadable_voucher_value),
    }
}

fn decimal_digits(value: u128) -> usize {
    value.to_string().len()
}

fn selection_error_kind(error: &CoinSelectionError) -> &'static str {
    match error {
        CoinSelectionError::ZeroAmount => "zero_amount",
        CoinSelectionError::EmptyWallet => "empty_wallet",
        CoinSelectionError::AmountNotRepresentable { .. } => "amount_not_representable",
        CoinSelectionError::NoReadyVouchers => "vouchers_not_ready",
        CoinSelectionError::TooManyVouchersInGroup { .. } => "too_many_vouchers",
        CoinSelectionError::InsufficientFunds => "insufficient_funds",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegularTransferError {
    Selection(CoinSelectionError),
    PreviewNotFound,
    PreviewExpired,
    BalanceChanged,
    NonDegradedAmountUnavailable,
    OperationNotFound,
    InvalidOperationId,
    Planning(String),
    Reservation(String),
    Journal(String),
    HandoffRejected,
}

impl std::fmt::Display for RegularTransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selection(error) => error.fmt(formatter),
            Self::PreviewNotFound => formatter.write_str("transfer preview was not found"),
            Self::PreviewExpired => formatter.write_str("transfer preview expired"),
            Self::BalanceChanged => {
                formatter.write_str("CASH balance changed; preview the payment again")
            }
            Self::NonDegradedAmountUnavailable => {
                formatter.write_str("no non-degraded CASH amount is available")
            }
            Self::OperationNotFound => formatter.write_str("transfer operation was not found"),
            Self::InvalidOperationId => {
                formatter.write_str("transfer operation identifier is empty")
            }
            Self::Planning(error) => write!(formatter, "transfer planning failed: {error}"),
            Self::Reservation(error) => write!(formatter, "transfer reservation failed: {error}"),
            Self::Journal(error) => write!(formatter, "transfer journal failed: {error}"),
            Self::HandoffRejected => {
                formatter.write_str("CASH message was rejected before durable acceptance")
            }
        }
    }
}

impl std::error::Error for RegularTransferError {}

impl From<CoinSelectionError> for RegularTransferError {
    fn from(value: CoinSelectionError) -> Self {
        Self::Selection(value)
    }
}

/// One split submission after recipient/change indices have been allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitTransferSubmission {
    pub overflow_coin: Coin,
    pub recipient_coins: Vec<Coin>,
    pub change_coins: Vec<Coin>,
    pub wal_entry_id: String,
}

/// One unload group before the host resolves the shared finalized state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloadGroupDraft {
    pub recycler: RecyclerKey,
    pub vouchers: Vec<Voucher>,
    pub recipient_coins: Vec<Coin>,
    pub change_coins: Vec<Coin>,
    pub wal_entry_id: String,
}

/// Chain-origin inputs resolved at one finalized snapshot. Proof bytes are
/// deliberately absent: they are created only after the final transaction
/// implication exists inside the submitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloadOriginPreparation {
    pub recycler_ring: RingProofParams,
    pub person_origin: PersonOriginKind,
    pub people_ring: RingProofParams,
    pub token: ResolvedUnloadToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUnloadGroup {
    pub draft: UnloadGroupDraft,
    pub readiness_block_hash: [u8; 32],
    pub recycler_revision: u32,
    pub origin: UnloadOriginPreparation,
}

/// Production implementations must update the named WAL checkpoint after
/// transaction creation and before broadcast. A successful return certifies
/// that the submitted input consumption and exact requested recipient/change
/// allocation finalized successfully. This evidence permits retiring the
/// child WAL even if the recipient consumes an output before the next query.
#[async_trait]
pub trait RegularTransferSubmitter: Send + Sync {
    async fn prepare_unload_groups(
        &self,
        groups: &[UnloadGroupDraft],
    ) -> Result<Vec<PreparedUnloadGroup>, String>;

    async fn submit_split(&self, submission: &SplitTransferSubmission) -> Result<(), String>;

    async fn submit_unload_group(&self, submission: &PreparedUnloadGroup) -> Result<(), String>;
}

pub struct RegularCoinTransferParts {
    /// Executor owned by the active signing authority.
    pub spawner: crate::Spawner,
    pub coins: Arc<dyn CoinRepository>,
    pub vouchers: Arc<dyn VoucherRepository>,
    pub wal: Arc<dyn WalStore>,
    pub allocator: Arc<CoinAllocator>,
    pub submitter: Arc<dyn RegularTransferSubmitter>,
    pub denominations: DenominationBreakdownContext,
    pub max_consolidation: usize,
    pub clock: Arc<dyn Clock>,
    pub session_generation: u64,
    pub committer: Option<Arc<dyn TransferStateCommitter>>,
}

#[derive(Clone)]
struct CachedPreview {
    descriptor: OpaqueTransferPreview,
    full: CoinSelectionResult,
    non_degraded: Option<CoinSelectionResult>,
    coins: Vec<Coin>,
    vouchers: Vec<Voucher>,
}

#[derive(Default)]
struct Confirmation {
    result: Mutex<Option<Result<(), RegularTransferError>>>,
    notify: Notify,
}

impl Confirmation {
    async fn wait(&self) -> Result<(), RegularTransferError> {
        loop {
            let notified = self.notify.notified();
            if let Some(result) = self.result.lock().clone() {
                return result;
            }
            notified.await;
        }
    }

    fn finish(&self, result: Result<(), RegularTransferError>) {
        *self.result.lock() = Some(result);
        self.notify.notify_waiters();
    }
}

pub struct RegularCoinTransferService {
    spawner: crate::Spawner,
    key_factory: Arc<CoinKeypairFactory>,
    coins: Arc<dyn CoinRepository>,
    vouchers: Arc<dyn VoucherRepository>,
    wal: Arc<dyn WalStore>,
    allocator: Arc<CoinAllocator>,
    submitter: Arc<dyn RegularTransferSubmitter>,
    denominations: DenominationBreakdownContext,
    max_consolidation: usize,
    clock: Arc<dyn Clock>,
    session_generation: u64,
    committer: Option<Arc<dyn TransferStateCommitter>>,
    previews: AsyncMutex<HashMap<String, CachedPreview>>,
    confirmations: AsyncMutex<HashMap<String, Arc<Confirmation>>>,
    execution_lock: AsyncMutex<()>,
}

impl RegularCoinTransferService {
    pub fn new(root_entropy: &[u8], parts: RegularCoinTransferParts) -> Self {
        Self {
            key_factory: Arc::new(CoinKeypairFactory::new(root_entropy)),
            coins: parts.coins,
            vouchers: parts.vouchers,
            wal: parts.wal,
            allocator: parts.allocator,
            submitter: parts.submitter,
            denominations: parts.denominations,
            max_consolidation: parts.max_consolidation.max(1),
            clock: parts.clock,
            session_generation: parts.session_generation,
            committer: parts.committer,
            previews: AsyncMutex::new(HashMap::new()),
            confirmations: AsyncMutex::new(HashMap::new()),
            execution_lock: AsyncMutex::new(()),
            spawner: parts.spawner,
        }
    }

    /// The native send UI speaks whole CASH cents; selection and memo values
    /// speak raw chain planks. Keep their only conversion at this service
    /// boundary so callers cannot accidentally preview cents as planks.
    pub fn cash_cents_to_planks(&self, cents: u128) -> Option<u128> {
        self.denominations.cash_cents_to_planks(cents)
    }

    pub fn cash_cents_from_planks(&self, planks: u128) -> Option<u128> {
        self.denominations.cash_cents_from_planks(planks)
    }

    pub fn planks_per_cash_cent(&self) -> u128 {
        self.denominations.asset_unit
    }

    pub async fn preview(
        &self,
        amount: u128,
    ) -> Result<OpaqueTransferPreview, RegularTransferError> {
        let coins = self
            .coins
            .list()
            .await
            .map_err(RegularTransferError::Planning)?;
        let vouchers = self
            .vouchers
            .list()
            .await
            .map_err(RegularTransferError::Planning)?;
        let now_ms = self.clock.now_ms();
        let full = match CoinSelector::new(self.denominations.clone(), self.max_consolidation)
            .select(amount, &coins, &vouchers, now_ms)
        {
            Ok(selection) => selection,
            Err(selection_error) => {
                let snapshot =
                    selector_snapshot_diagnostic(&coins, &vouchers, &self.denominations, now_ms);
                warn!(
                    error = selection_error_kind(&selection_error),
                    strategy = "none",
                    requested_value_digits = decimal_digits(amount),
                    selectable_coins = snapshot.selectable_coins,
                    expiring_coins = snapshot.expiring_coins,
                    locked_coins = snapshot.locked_coins,
                    unloadable_vouchers = snapshot.unloadable_vouchers,
                    locked_vouchers = snapshot.locked_vouchers,
                    full_vouchers = snapshot.full_vouchers,
                    degraded_vouchers = snapshot.degraded_vouchers,
                    selectable_coin_value_digits = snapshot.selectable_coin_value_digits,
                    unloadable_voucher_value_digits = snapshot.unloadable_voucher_value_digits,
                    "coinage selector snapshot"
                );
                return Err(selection_error.into());
            }
        };
        let (non_degraded, non_degraded_amount) =
            non_degraded_selection(&full, &self.denominations, now_ms);
        let strategy = match full.strategy {
            TransferStrategy::ExactMatch { .. } => TransferPreviewStrategy::ExactMatch,
            TransferStrategy::Split { .. } => TransferPreviewStrategy::Split,
            TransferStrategy::UnloadIntoCoins { .. } => TransferPreviewStrategy::UnloadIntoCoins,
        };
        let preview_id = random_id("cash-preview");
        let descriptor = OpaqueTransferPreview {
            preview_id: preview_id.clone(),
            session_generation: self.session_generation,
            full_amount: amount,
            max_debit_amount: selection_max_debit(&full, &self.denominations),
            non_degraded_amount,
            is_degraded: full.privacy_level == PrivacyLevel::Degraded,
            strategy,
            expires_at_ms: now_ms.saturating_add(PREVIEW_LIFETIME_MS),
        };
        let mut previews = self.previews.lock().await;
        previews.retain(|_, preview| preview.descriptor.expires_at_ms >= now_ms);
        previews.insert(
            preview_id,
            CachedPreview {
                descriptor: descriptor.clone(),
                full,
                non_degraded,
                coins,
                vouchers,
            },
        );
        Ok(descriptor)
    }

    pub async fn preview_descriptor(
        &self,
        preview_id: &str,
    ) -> Result<OpaqueTransferPreview, RegularTransferError> {
        let descriptor = match self
            .previews
            .lock()
            .await
            .get(preview_id)
            .map(|preview| preview.descriptor.clone())
        {
            Some(descriptor) => descriptor,
            None => {
                return Err(RegularTransferError::PreviewNotFound);
            }
        };
        if self.clock.now_ms() > descriptor.expires_at_ms {
            return Err(RegularTransferError::PreviewExpired);
        }
        Ok(descriptor)
    }

    /// Confirms exactly the cached selector result. Calls racing for the same
    /// preview join one result; only the first closure can receive the memo.
    /// `handoff` may return `Err(())` only if it certifies that no recipient or
    /// transport accepted the secret. Ambiguous delivery must retain the WAL
    /// by returning `Ok(())` and reporting transport uncertainty separately.
    pub async fn confirm<F, Fut>(
        self: &Arc<Self>,
        preview_id: &str,
        choice: TransferPreviewChoice,
        handoff: F,
    ) -> Result<(), RegularTransferError>
    where
        F: FnOnce(TransferMemo) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), ()>> + Send + 'static,
    {
        let (confirmation, leader) = {
            let mut confirmations = self.confirmations.lock().await;
            match confirmations.get(preview_id) {
                Some(existing) => (Arc::clone(existing), false),
                None => {
                    let confirmation = Arc::new(Confirmation::default());
                    confirmations.insert(preview_id.to_owned(), Arc::clone(&confirmation));
                    (confirmation, true)
                }
            }
        };
        if leader {
            let service = Arc::clone(self);
            let preview_id = preview_id.to_owned();
            let running = Arc::clone(&confirmation);
            crate::tasks::spawn_abortable(&self.spawner, async move {
                let result = service
                    .confirm_operation(&preview_id, &preview_id, choice, handoff)
                    .await;
                running.finish(result);
            });
        }
        confirmation.wait().await
    }

    /// Confirms a host-owned durable operation. The host must serialize wallet
    /// recovery with this call and run it on its cancellation-safe executor.
    /// The callback may await durable transport persistence before returning.
    /// `Err` from the callback certifies no acceptance; uncertain acceptance
    /// must return `Ok` and retain the transport's idempotent operation key.
    ///
    /// All chain work finishes (or leaves recoverable WAL) before returning.
    /// Once transport accepts, chain/persistence failures never become `Err`.
    pub async fn confirm_operation<F, Fut>(
        self: &Arc<Self>,
        operation_id: &str,
        preview_id: &str,
        choice: TransferPreviewChoice,
        handoff: F,
    ) -> Result<(), RegularTransferError>
    where
        F: FnOnce(TransferMemo) -> Fut + Send,
        Fut: Future<Output = Result<(), ()>> + Send,
    {
        if operation_id.is_empty() {
            return Err(RegularTransferError::InvalidOperationId);
        }
        let _execution = self.execution_lock.lock().await;
        let existing = self.operation_wal(operation_id).await?;
        if !existing.is_empty() {
            return self.resume_entries(existing, handoff).await;
        }
        let cached = self
            .previews
            .lock()
            .await
            .get(preview_id)
            .cloned()
            .ok_or(RegularTransferError::PreviewNotFound)?;
        if self.clock.now_ms() > cached.descriptor.expires_at_ms {
            return Err(RegularTransferError::PreviewExpired);
        }
        let current_coins = self
            .coins
            .list()
            .await
            .map_err(RegularTransferError::Planning)?;
        let current_vouchers = self
            .vouchers
            .list()
            .await
            .map_err(RegularTransferError::Planning)?;
        if current_coins != cached.coins || current_vouchers != cached.vouchers {
            return Err(RegularTransferError::BalanceChanged);
        }
        let expected_amount = match choice {
            TransferPreviewChoice::Full => cached.descriptor.full_amount,
            TransferPreviewChoice::NonDegraded => cached.descriptor.non_degraded_amount,
        };
        let result = match choice {
            TransferPreviewChoice::Full => cached.full,
            TransferPreviewChoice::NonDegraded => cached
                .non_degraded
                .ok_or(RegularTransferError::NonDegradedAmountUnavailable)?,
        };
        let amount = selection_value(&result, &self.denominations);
        if amount == 0
            || amount != expected_amount
            || selection_max_debit(&result, &self.denominations)
                > cached.descriptor.max_debit_amount
        {
            return Err(RegularTransferError::BalanceChanged);
        }
        let mut plan = self.create_plan(operation_id, result, amount).await?;
        let context = self.transfer_context();
        context
            .reserve(&plan.reserved_coins, &plan.reserved_vouchers)
            .await
            .map_err(RegularTransferError::Reservation)?;
        if let Err(error) = self.wal.save_all(&plan.journals).await {
            // A failed acknowledgement can still have committed the atomic
            // batch. Never partially delete it: restart would lose the plan.
            // Recovery restores orphan reservations if no batch committed,
            // otherwise the complete Prepared operation remains resumable.
            return Err(RegularTransferError::Journal(error));
        }
        let mut parent = plan
            .journals
            .iter()
            .find(|entry| entry.operation.is_transfer_receipt())
            .cloned()
            .expect("operation plan includes its durable receipt");
        let memo = plan.memo.take().expect("plan memo consumed once");
        if handoff(memo).await.is_err() {
            self.reject_operation(&mut parent, &plan.journals).await?;
            return Err(RegularTransferError::HandoffRejected);
        }
        if let Err(error) = self
            .finish_accepted(&mut parent, &plan.journals, plan.chain)
            .await
        {
            error!(%error, "accepted CASH operation remains journaled for recovery");
        }
        Ok(())
    }

    pub async fn operation_wal(
        &self,
        operation_id: &str,
    ) -> Result<Vec<TransferWalEntry>, RegularTransferError> {
        self.wal
            .load_operation(operation_id)
            .await
            .map_err(RegularTransferError::Journal)
    }

    /// Resumes only existing allocations, never selects or debits new inputs.
    /// Invoke after recovery and renewed user approval, never from passive
    /// reconciliation. For Prepared receipts, `handoff` must resolve/replay the
    /// SAME host transport operation, since acceptance may precede a crash.
    /// Accepted/Completed receipts never invoke it. Known checkpoints are not
    /// broadcast again; recovery must first prove their mortality or fork.
    pub async fn resume_operation<F, Fut>(
        self: &Arc<Self>,
        operation_id: &str,
        handoff: F,
    ) -> Result<(), RegularTransferError>
    where
        F: FnOnce(TransferMemo) -> Fut + Send,
        Fut: Future<Output = Result<(), ()>> + Send,
    {
        let _execution = self.execution_lock.lock().await;
        self.resume_entries(self.operation_wal(operation_id).await?, handoff)
            .await
    }

    async fn resume_entries<F, Fut>(
        &self,
        entries: Vec<TransferWalEntry>,
        handoff: F,
    ) -> Result<(), RegularTransferError>
    where
        F: FnOnce(TransferMemo) -> Fut + Send,
        Fut: Future<Output = Result<(), ()>> + Send,
    {
        let mut parent = entries
            .iter()
            .find(|entry| entry.operation.is_transfer_receipt())
            .cloned()
            .ok_or(RegularTransferError::OperationNotFound)?;
        match parent.operation {
            WalOperation::TransferCompleted => return Ok(()),
            WalOperation::TransferRejected => {
                self.reject_operation(&mut parent, &entries).await?;
                return Err(RegularTransferError::HandoffRejected);
            }
            WalOperation::TransferPrepared => {
                let coins = parent
                    .payload
                    .output_coins
                    .iter()
                    .map(referenced_coin)
                    .collect::<Vec<_>>();
                let amount = coins.iter().fold(0u128, |sum, coin| {
                    sum.saturating_add(self.denominations.value_in_planks(coin.exponent))
                });
                let memo = self.build_memo(&coins, amount)?;
                if handoff(memo).await.is_err() {
                    self.reject_operation(&mut parent, &entries).await?;
                    return Err(RegularTransferError::HandoffRejected);
                }
            }
            WalOperation::TransferAccepted => {}
            _ => return Err(RegularTransferError::OperationNotFound),
        }
        // From here the memo may already be owned by the recipient.
        let completion = async {
            parent.operation = WalOperation::TransferAccepted;
            self.wal.save(&parent).await?;
            let chain = self.restore_chain_plan(&entries).await?;
            self.finish_accepted(&mut parent, &entries, chain).await
        }
        .await;
        if let Err(error) = completion {
            error!(%error, "accepted CASH operation remains journaled for recovery");
        }
        Ok(())
    }

    fn transfer_context(&self) -> Arc<TransferContext> {
        let mut context = TransferContext::new(Arc::clone(&self.coins), Arc::clone(&self.vouchers));
        if let Some(committer) = &self.committer {
            context = context.with_committer(Arc::clone(committer));
        }
        Arc::new(context)
    }

    async fn reject_operation(
        &self,
        parent: &mut TransferWalEntry,
        entries: &[TransferWalEntry],
    ) -> Result<(), RegularTransferError> {
        parent.operation = WalOperation::TransferRejected;
        self.wal
            .save(parent)
            .await
            .map_err(RegularTransferError::Journal)?;
        for entry in entries
            .iter()
            .filter(|entry| !entry.operation.is_transfer_receipt())
        {
            for input in &entry.payload.input_coins {
                self.coins
                    .set_state(input.derivation_index, CoinState::Available)
                    .await
                    .map_err(RegularTransferError::Reservation)?;
            }
            for input in &entry.payload.input_vouchers {
                self.vouchers
                    .set_local_state(
                        input.derivation_index,
                        crate::model::VoucherLocalState::Available,
                    )
                    .await
                    .map_err(RegularTransferError::Reservation)?;
            }
            self.wal
                .delete(&entry.entry_id)
                .await
                .map_err(RegularTransferError::Journal)?;
        }
        Ok(())
    }

    async fn finish_accepted(
        &self,
        parent: &mut TransferWalEntry,
        entries: &[TransferWalEntry],
        chain: ChainPlan,
    ) -> Result<(), String> {
        parent.operation = WalOperation::TransferAccepted;
        self.wal.save(parent).await?;
        let context = self.transfer_context();
        for entry in entries
            .iter()
            .filter(|entry| entry.operation == WalOperation::SecretHandoff)
        {
            let indices = entry
                .payload
                .input_coins
                .iter()
                .map(|coin| coin.derivation_index)
                .collect::<Vec<_>>();
            context.process_outputs(&indices, &[], &[], &[]).await?;
            self.wal.delete(&entry.entry_id).await?;
        }
        self.run_chain_plan(chain, context).await?;
        let children_remain = self.wal.load_all().await?.iter().any(|entry| {
            entry.entry_id != parent.entry_id
                && entry.operation_parent_id().as_deref() == Some(parent.entry_id.as_str())
        });
        if !children_remain {
            parent.operation = WalOperation::TransferCompleted;
            self.wal.save(parent).await?;
        }
        Ok(())
    }

    async fn restore_chain_plan(&self, entries: &[TransferWalEntry]) -> Result<ChainPlan, String> {
        let mut drafts = Vec::new();
        let vouchers = if entries.iter().any(|entry| {
            entry.operation == WalOperation::IntoCoins
                && entry.checkpoint == CheckpointBlock::Pending
        }) {
            self.vouchers.list().await?
        } else {
            Vec::new()
        };
        for entry in entries.iter().filter(|entry| {
            entry.checkpoint == CheckpointBlock::Pending
                && matches!(
                    entry.operation,
                    WalOperation::Split | WalOperation::IntoCoins
                )
        }) {
            let recipient_coins = entry
                .payload
                .destination_coins
                .iter()
                .map(referenced_coin)
                .collect::<Vec<_>>();
            let change_coins = entry
                .payload
                .output_coins
                .iter()
                .filter(|coin| !entry.payload.destination_coins.contains(coin))
                .map(referenced_coin)
                .collect::<Vec<_>>();
            match entry.operation {
                WalOperation::Split => {
                    if entry.payload.input_coins.len() != 1 {
                        return Err("split journal does not have exactly one input".into());
                    }
                    return Ok(ChainPlan::Split(SplitTransferSubmission {
                        overflow_coin: referenced_coin(&entry.payload.input_coins[0]),
                        recipient_coins,
                        change_coins,
                        wal_entry_id: entry.entry_id.clone(),
                    }));
                }
                WalOperation::IntoCoins => {
                    let members = entry
                        .payload
                        .input_vouchers
                        .iter()
                        .map(|reference| {
                            vouchers
                                .iter()
                                .find(|voucher| {
                                    voucher.derivation_index == reference.derivation_index
                                        && voucher.exponent == reference.exponent
                                })
                                .cloned()
                                .ok_or_else(|| {
                                    "unload journal input is not available locally".to_string()
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let first = members.first().ok_or("unload journal has no vouchers")?;
                    let VoucherRemoteState::InRecycler { recycler_index } = first.remote_state
                    else {
                        return Err("unload journal awaits voucher location reconciliation".into());
                    };
                    let recycler = RecyclerKey {
                        exponent: first.exponent,
                        index: recycler_index,
                    };
                    if members.iter().any(|voucher| {
                        voucher.exponent != recycler.exponent
                            || voucher.remote_state != first.remote_state
                    }) {
                        return Err("unload journal spans multiple recyclers".into());
                    }
                    drafts.push(UnloadGroupDraft {
                        recycler,
                        vouchers: members,
                        recipient_coins,
                        change_coins,
                        wal_entry_id: entry.entry_id.clone(),
                    });
                }
                _ => {}
            }
        }
        if drafts.is_empty() {
            return Ok(ChainPlan::None);
        }
        let prepared = self.submitter.prepare_unload_groups(&drafts).await?;
        if prepared.len() != drafts.len()
            || prepared
                .iter()
                .zip(&drafts)
                .any(|(prepared, draft)| &prepared.draft != draft)
        {
            return Err("unload recovery changed the durable allocation".into());
        }
        Ok(ChainPlan::Unload(prepared))
    }

    async fn create_plan(
        &self,
        operation_id: &str,
        result: CoinSelectionResult,
        amount: u128,
    ) -> Result<TransferPlan, RegularTransferError> {
        let mut journals = Vec::new();
        let mut reserved_coins = Vec::new();
        let mut reserved_vouchers = Vec::new();
        let (pass_through, recipient, chain) = match result.strategy {
            TransferStrategy::ExactMatch { coins } => {
                reserved_coins.extend(coins.iter().map(|coin| coin.derivation_index));
                (coins.clone(), coins, ChainPlan::None)
            }
            TransferStrategy::Split {
                partial_coins,
                split_coin,
                target_denominations,
                change_denominations,
            } => {
                let recipient = self.allocate_coins(&target_denominations).await?;
                let change = self.allocate_coins(&change_denominations).await?;
                reserved_coins.extend(partial_coins.iter().map(|coin| coin.derivation_index));
                reserved_coins.push(split_coin.derivation_index);
                let wal_entry_id = operation_entry_id(operation_id, "split");
                let submission = SplitTransferSubmission {
                    overflow_coin: split_coin.clone(),
                    recipient_coins: recipient.clone(),
                    change_coins: change.clone(),
                    wal_entry_id: wal_entry_id.clone(),
                };
                journals.push(chain_journal(
                    wal_entry_id,
                    WalOperation::Split,
                    &[split_coin],
                    &[],
                    &recipient,
                    &change,
                    self.clock.now_ms(),
                ));
                let mut memo_coins = partial_coins.clone();
                memo_coins.extend(recipient);
                (partial_coins, memo_coins, ChainPlan::Split(submission))
            }
            TransferStrategy::UnloadIntoCoins { coins, groups } => {
                reserved_coins.extend(coins.iter().map(|coin| coin.derivation_index));
                let mut drafts = Vec::with_capacity(groups.len());
                let mut memo_coins = coins.clone();
                for (group_index, group) in groups.into_iter().enumerate() {
                    let recipient = self.allocate_coins(&group.recipient_denominations).await?;
                    let change = self.allocate_coins(&group.change_denominations).await?;
                    reserved_vouchers.extend(
                        group
                            .vouchers
                            .iter()
                            .map(|voucher| voucher.derivation_index),
                    );
                    memo_coins.extend(recipient.clone());
                    drafts.push(UnloadGroupDraft {
                        recycler: group.recycler,
                        vouchers: group.vouchers,
                        recipient_coins: recipient,
                        change_coins: change,
                        wal_entry_id: operation_entry_id(
                            operation_id,
                            &format!("unload-{group_index}"),
                        ),
                    });
                }
                let prepared = self
                    .submitter
                    .prepare_unload_groups(&drafts)
                    .await
                    .map_err(|error| RegularTransferError::Planning(error))?;
                if prepared.len() != drafts.len()
                    || prepared
                        .iter()
                        .zip(&drafts)
                        .any(|(prepared, draft)| &prepared.draft != draft)
                {
                    return Err(RegularTransferError::Planning(
                        "unload preparation did not preserve every recycler group".into(),
                    ));
                }
                for group in &prepared {
                    journals.push(chain_journal(
                        group.draft.wal_entry_id.clone(),
                        WalOperation::IntoCoins,
                        &[],
                        &group.draft.vouchers,
                        &group.draft.recipient_coins,
                        &group.draft.change_coins,
                        self.clock.now_ms(),
                    ));
                }
                (coins, memo_coins, ChainPlan::Unload(prepared))
            }
        };

        let memo = self.build_memo(&recipient, amount)?;
        if !pass_through.is_empty() {
            let id = operation_entry_id(operation_id, "handoff");
            journals.push(TransferWalEntry {
                entry_id: id.clone(),
                operation: WalOperation::SecretHandoff,
                payload: WalPayload {
                    input_coins: coin_refs(&pass_through),
                    ..WalPayload::default()
                },
                checkpoint: CheckpointBlock::Pending,
                created_at_ms: self.clock.now_ms(),
            });
        }
        journals.push(TransferWalEntry {
            entry_id: operation_entry_id(operation_id, "parent"),
            operation: WalOperation::TransferPrepared,
            payload: WalPayload {
                output_coins: coin_refs(&recipient),
                ..WalPayload::default()
            },
            checkpoint: CheckpointBlock::Pending,
            created_at_ms: self.clock.now_ms(),
        });
        Ok(TransferPlan {
            memo: Some(memo),
            reserved_coins,
            reserved_vouchers,
            journals,
            chain,
        })
    }

    async fn allocate_coins(
        &self,
        denominations: &[crate::denomination::Denomination],
    ) -> Result<Vec<Coin>, RegularTransferError> {
        let mut coins = Vec::with_capacity(denominations.len());
        for denomination in denominations {
            coins.push(
                self.allocator
                    .allocate(denomination.exponent)
                    .await
                    .map_err(RegularTransferError::Planning)?,
            );
        }
        Ok(coins)
    }

    fn build_memo(
        &self,
        coins: &[Coin],
        expected: u128,
    ) -> Result<TransferMemo, RegularTransferError> {
        let total = coins.iter().fold(0u128, |total, coin| {
            total.saturating_add(self.denominations.value_in_planks(coin.exponent))
        });
        if total != expected {
            return Err(RegularTransferError::Planning(format!(
                "memo value {total} does not equal selected amount {expected}"
            )));
        }
        let entries = coins
            .iter()
            .map(|coin| {
                self.key_factory
                    .secret_bytes(coin.derivation_index)
                    .map(MemoEntry)
                    .map_err(RegularTransferError::Planning)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TransferMemo {
            entries,
            total_value: total,
        })
    }

    async fn run_chain_plan(
        &self,
        chain: ChainPlan,
        context: Arc<TransferContext>,
    ) -> Result<(), String> {
        match chain {
            ChainPlan::None => Ok(()),
            ChainPlan::Split(submission) => match self.submitter.submit_split(&submission).await {
                Ok(()) => {
                    context
                        .process_outputs(
                            &[submission.overflow_coin.derivation_index],
                            &[],
                            &submission.change_coins,
                            &submission.recipient_coins,
                        )
                        .await?;
                    self.wal.delete(&submission.wal_entry_id).await
                }
                // Even a pre-broadcast failure must retain the fixed outputs:
                // their secrets already belong to the recipient.
                Err(error) => Err(error),
            },
            ChainPlan::Unload(groups) => {
                let outcomes = join_all(groups.into_iter().map(|group| {
                    let submitter = Arc::clone(&self.submitter);
                    async move {
                        let result = submitter.submit_unload_group(&group).await;
                        (group, result)
                    }
                }))
                .await;
                let mut errors = Vec::new();
                for (group, outcome) in outcomes {
                    let voucher_ids = group
                        .draft
                        .vouchers
                        .iter()
                        .map(|voucher| voucher.derivation_index)
                        .collect::<Vec<_>>();
                    match outcome {
                        Ok(()) => {
                            if let Err(error) = context
                                .process_outputs(
                                    &[],
                                    &voucher_ids,
                                    &group.draft.change_coins,
                                    &group.draft.recipient_coins,
                                )
                                .await
                            {
                                errors.push(error);
                                continue;
                            }
                            if let Err(error) = self.wal.delete(&group.draft.wal_entry_id).await {
                                errors.push(error);
                            }
                        }
                        Err(error) => errors.push(error),
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.join("; "))
                }
            }
        }
    }
}

struct TransferPlan {
    memo: Option<TransferMemo>,
    reserved_coins: Vec<u32>,
    reserved_vouchers: Vec<u32>,
    journals: Vec<TransferWalEntry>,
    chain: ChainPlan,
}

enum ChainPlan {
    None,
    Split(SplitTransferSubmission),
    Unload(Vec<PreparedUnloadGroup>),
}

fn random_id(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("{prefix}-{}", hex::encode(bytes))
}

fn coin_refs(coins: &[Coin]) -> Vec<WalCoinRef> {
    coins
        .iter()
        .map(|coin| WalCoinRef {
            derivation_index: coin.derivation_index,
            exponent: coin.exponent,
        })
        .collect()
}

fn voucher_refs(vouchers: &[Voucher]) -> Vec<WalCoinRef> {
    vouchers
        .iter()
        .map(|voucher| WalCoinRef {
            derivation_index: voucher.derivation_index,
            exponent: voucher.exponent,
        })
        .collect()
}

fn referenced_coin(reference: &WalCoinRef) -> Coin {
    Coin {
        derivation_index: reference.derivation_index,
        exponent: reference.exponent,
        age: None,
        state: CoinState::PendingTransfer,
    }
}

fn chain_journal(
    entry_id: String,
    operation: WalOperation,
    input_coins: &[Coin],
    input_vouchers: &[Voucher],
    destination: &[Coin],
    change: &[Coin],
    created_at_ms: i64,
) -> TransferWalEntry {
    let mut output_coins = coin_refs(destination);
    output_coins.extend(coin_refs(change));
    TransferWalEntry {
        entry_id,
        operation,
        payload: WalPayload {
            input_coins: coin_refs(input_coins),
            input_vouchers: voucher_refs(input_vouchers),
            output_coins,
            output_vouchers: Vec::new(),
            destination_coins: coin_refs(destination),
        },
        checkpoint: CheckpointBlock::Pending,
        created_at_ms,
    }
}

fn selection_value(result: &CoinSelectionResult, context: &DenominationBreakdownContext) -> u128 {
    match &result.strategy {
        TransferStrategy::ExactMatch { coins } => coins.iter().fold(0u128, |sum, coin| {
            sum.saturating_add(context.value_in_planks(coin.exponent))
        }),
        TransferStrategy::Split {
            partial_coins,
            target_denominations,
            ..
        } => partial_coins
            .iter()
            .fold(0u128, |sum, coin| {
                sum.saturating_add(context.value_in_planks(coin.exponent))
            })
            .saturating_add(context.total_value(target_denominations)),
        TransferStrategy::UnloadIntoCoins { coins, groups } => groups.iter().fold(
            coins.iter().fold(0u128, |sum, coin| {
                sum.saturating_add(context.value_in_planks(coin.exponent))
            }),
            |sum, group| sum.saturating_add(context.total_value(&group.recipient_denominations)),
        ),
    }
}

fn selection_max_debit(
    result: &CoinSelectionResult,
    context: &DenominationBreakdownContext,
) -> u128 {
    match &result.strategy {
        TransferStrategy::UnloadIntoCoins { coins, groups } => coins
            .iter()
            .map(|coin| coin.exponent)
            .chain(
                groups
                    .iter()
                    .flat_map(|group| group.vouchers.iter().map(|voucher| voucher.exponent)),
            )
            .fold(0u128, |total, exponent| {
                total.saturating_add(context.value_in_planks(exponent))
            }),
        // Whole coins and splits retain their planned change exactly.
        _ => selection_value(result, context),
    }
}

fn non_degraded_selection(
    full: &CoinSelectionResult,
    context: &DenominationBreakdownContext,
    now_ms: i64,
) -> (Option<CoinSelectionResult>, u128) {
    if full.privacy_level == PrivacyLevel::Full {
        return (Some(full.clone()), selection_value(full, context));
    }
    let TransferStrategy::UnloadIntoCoins { coins, groups } = &full.strategy else {
        return (Some(full.clone()), selection_value(full, context));
    };
    let groups = groups
        .iter()
        .filter(|group| {
            group
                .vouchers
                .iter()
                .all(|voucher| voucher.effective_privacy(now_ms) == EffectivePrivacy::Full)
        })
        .cloned()
        .collect::<Vec<_>>();
    let result = CoinSelectionResult {
        strategy: if groups.is_empty() {
            TransferStrategy::ExactMatch {
                coins: coins.clone(),
            }
        } else {
            TransferStrategy::UnloadIntoCoins {
                coins: coins.clone(),
                groups,
            }
        },
        privacy_level: PrivacyLevel::Full,
    };
    let amount = selection_value(&result, context);
    ((amount > 0).then_some(result), amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use crate::balance::compute_balance;
    use crate::clock::FixedClock;
    use crate::index_store::InMemoryCoinageIndexStore;
    use crate::model::{CoinState, VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState};
    use crate::repo::{InMemoryCoinRepository, InMemoryVoucherRepository};

    #[derive(Default)]
    struct MemWal(Mutex<Vec<TransferWalEntry>>);

    #[async_trait]
    impl WalStore for MemWal {
        async fn save(&self, entry: &TransferWalEntry) -> Result<(), String> {
            let mut entries = self.0.lock();
            entries.retain(|existing| existing.entry_id != entry.entry_id);
            entries.push(entry.clone());
            Ok(())
        }
        async fn save_all(&self, entries: &[TransferWalEntry]) -> Result<(), String> {
            let mut stored = self.0.lock();
            stored.retain(|existing| {
                !entries
                    .iter()
                    .any(|entry| entry.entry_id == existing.entry_id)
            });
            stored.extend_from_slice(entries);
            Ok(())
        }
        async fn update_checkpoint(
            &self,
            entry_id: &str,
            checkpoint: CheckpointBlock,
        ) -> Result<(), String> {
            let mut entries = self.0.lock();
            entries
                .iter_mut()
                .find(|entry| entry.entry_id == entry_id)
                .ok_or_else(|| "missing WAL".to_string())?
                .checkpoint = checkpoint;
            Ok(())
        }
        async fn load_all(&self) -> Result<Vec<TransferWalEntry>, String> {
            Ok(self.0.lock().clone())
        }
        async fn delete(&self, entry_id: &str) -> Result<(), String> {
            self.0.lock().retain(|entry| entry.entry_id != entry_id);
            Ok(())
        }
    }

    struct MockSubmitter {
        wal: Arc<MemWal>,
        fail_unload: HashSet<u32>,
    }

    #[async_trait]
    impl RegularTransferSubmitter for MockSubmitter {
        async fn prepare_unload_groups(
            &self,
            groups: &[UnloadGroupDraft],
        ) -> Result<Vec<PreparedUnloadGroup>, String> {
            Ok(groups
                .iter()
                .cloned()
                .map(|draft| PreparedUnloadGroup {
                    recycler_revision: 1,
                    readiness_block_hash: [7; 32],
                    origin: UnloadOriginPreparation {
                        recycler_ring: RingProofParams {
                            ring_exponent: 9,
                            ring_index: draft.recycler.index,
                            ring_revision: 1,
                            ring_members: vec![[1; 32]],
                        },
                        person_origin: PersonOriginKind::Lite,
                        people_ring: RingProofParams {
                            ring_exponent: 9,
                            ring_index: 2,
                            ring_revision: 1,
                            ring_members: vec![[2; 32]],
                        },
                        token: ResolvedUnloadToken {
                            period: 3,
                            counter: draft.recycler.index,
                        },
                    },
                    draft,
                })
                .collect())
        }

        async fn submit_split(&self, submission: &SplitTransferSubmission) -> Result<(), String> {
            self.wal
                .update_checkpoint(
                    &submission.wal_entry_id,
                    CheckpointBlock::Known {
                        number: 10,
                        hash: [9; 32],
                    },
                )
                .await
        }

        async fn submit_unload_group(
            &self,
            submission: &PreparedUnloadGroup,
        ) -> Result<(), String> {
            self.wal
                .update_checkpoint(
                    &submission.draft.wal_entry_id,
                    CheckpointBlock::Known {
                        number: 10,
                        hash: [9; 32],
                    },
                )
                .await?;
            if self.fail_unload.contains(&submission.draft.recycler.index) {
                Err("scripted ambiguous unload failure".into())
            } else {
                Ok(())
            }
        }
    }

    fn context() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 1,
            max_exponent: 10,
            min_exponent: 0,
            precision: 2,
        }
    }

    fn voucher(index: u32, exponent: i16, recycler: u32) -> Voucher {
        Voucher {
            exponent,
            derivation_index: index,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: VoucherRemoteState::InRecycler {
                recycler_index: recycler,
            },
            local_state: VoucherLocalState::Available,
            privacy: VoucherPrivacyLevel::Full,
        }
    }

    #[test]
    fn selector_snapshot_is_aggregate_only_and_uses_value_digit_counts() {
        let coins = vec![
            Coin {
                exponent: 2,
                derivation_index: 99,
                age: Some(1),
                state: CoinState::Available,
            },
            Coin {
                exponent: 1,
                derivation_index: 100,
                age: Some(14),
                state: CoinState::Available,
            },
            Coin {
                exponent: 0,
                derivation_index: 101,
                age: Some(1),
                state: CoinState::PendingTransfer,
            },
        ];
        let mut ready = voucher(7, 3, 4);
        ready.ready_at_ms = 0;
        let mut locked = voucher(8, 1, 4);
        locked.local_state = VoucherLocalState::PendingTransfer;
        locked.privacy = VoucherPrivacyLevel::Degraded;
        let diagnostic = selector_snapshot_diagnostic(&coins, &[ready, locked], &context(), 1_000);
        assert_eq!(diagnostic.selectable_coins, 1);
        assert_eq!(diagnostic.expiring_coins, 1);
        assert_eq!(diagnostic.locked_coins, 2);
        assert_eq!(diagnostic.unloadable_vouchers, 1);
        assert_eq!(diagnostic.locked_vouchers, 1);
        assert_eq!(diagnostic.full_vouchers, 1);
        assert_eq!(diagnostic.degraded_vouchers, 1);
        assert_eq!(diagnostic.selectable_coin_value_digits, 1); // value 4
        assert_eq!(diagnostic.unloadable_voucher_value_digits, 1); // value 8
    }

    fn service(
        vouchers: Vec<Voucher>,
        fail_unload: HashSet<u32>,
    ) -> (
        Arc<RegularCoinTransferService>,
        Arc<InMemoryCoinRepository>,
        Arc<InMemoryVoucherRepository>,
        Arc<MemWal>,
    ) {
        service_with_context(vouchers, fail_unload, context())
    }

    fn service_with_context(
        vouchers: Vec<Voucher>,
        fail_unload: HashSet<u32>,
        denominations: DenominationBreakdownContext,
    ) -> (
        Arc<RegularCoinTransferService>,
        Arc<InMemoryCoinRepository>,
        Arc<InMemoryVoucherRepository>,
        Arc<MemWal>,
    ) {
        let coins = Arc::new(InMemoryCoinRepository::default());
        let vouchers = Arc::new(InMemoryVoucherRepository::with_vouchers(vouchers));
        let wal = Arc::new(MemWal::default());
        let submitter = Arc::new(MockSubmitter {
            wal: Arc::clone(&wal),
            fail_unload,
        });
        let service = Arc::new(RegularCoinTransferService::new(
            &[0x33; 32],
            RegularCoinTransferParts {
                spawner: crate::test_spawner(),
                coins: Arc::clone(&coins) as Arc<_>,
                vouchers: Arc::clone(&vouchers) as Arc<_>,
                wal: Arc::clone(&wal) as Arc<_>,
                allocator: Arc::new(CoinAllocator::new(Arc::new(
                    InMemoryCoinageIndexStore::default(),
                ))),
                submitter,
                denominations,
                max_consolidation: 100,
                clock: Arc::new(FixedClock(1_000)),
                session_generation: 4,
                committer: None,
            },
        ));
        (service, coins, vouchers, wal)
    }

    /// Native amounts stay in CASH cents while the selector stays in raw
    /// asset planks. This regression covers real-scale CASH amounts and
    /// proves the 10,000x boundary is applied before all three selector
    /// strategies run.
    #[tokio::test]
    async fn native_cash_amounts_preview_at_six_decimal_asset_precision() {
        let denominations = DenominationBreakdownContext {
            asset_unit: 10_000,
            max_exponent: 14,
            min_exponent: 0,
            precision: 6,
        };
        // 2^14 + 2^11 + 2^10 + 2^9 + 2^5 = 20,000 cents = 200 CASH.
        let inventory = vec![
            voucher(1, 14, 1),
            voucher(2, 11, 2),
            voucher(3, 10, 3),
            voucher(4, 9, 4),
            voucher(5, 5, 5),
        ];
        let (service, _, _, _) = service_with_context(inventory, HashSet::new(), denominations);

        for (cash, cents, expected_planks) in [
            (1, 100, 1_000_000),
            (10, 1_000, 10_000_000),
            (100, 10_000, 100_000_000),
            (200, 20_000, 200_000_000),
        ] {
            let planks = service.cash_cents_to_planks(cents).unwrap();
            assert_eq!(planks, expected_planks, "{cash} CASH conversion");
            let preview = service.preview(planks).await.unwrap();
            assert_eq!(preview.full_amount, expected_planks, "{cash} CASH preview");
            assert_eq!(
                service.cash_cents_from_planks(preview.full_amount),
                Some(cents),
                "{cash} CASH projection"
            );
        }
    }

    #[tokio::test]
    async fn six_voucher_balance_previews_and_confirms_one_cash() {
        let inventory = vec![
            voucher(1, 10, 1), // 1024
            voucher(2, 4, 2),  // 16
            voucher(3, 4, 3),  // 16
            voucher(4, 3, 4),  // 8
            voucher(5, 1, 5),  // 2
            voucher(6, 1, 6),  // 2
        ];
        assert_eq!(
            compute_balance(&[], &inventory, &context(), 1_000).total_planks(),
            1_068
        );
        let (service, _coins, vouchers, wal) = service(inventory, HashSet::new());
        let preview = service.preview(100).await.unwrap();
        assert_eq!(preview.strategy, TransferPreviewStrategy::UnloadIntoCoins);
        assert_eq!(preview.full_amount, 100);
        service
            .confirm(
                &preview.preview_id,
                TransferPreviewChoice::Full,
                |memo| async move {
                    assert_eq!(memo.total_value, 100);
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert_eq!(vouchers.list().await.unwrap().len(), 5);
        assert_eq!(
            wal.load_operation(&preview.preview_id).await.unwrap()[0].operation,
            WalOperation::TransferCompleted
        );
    }

    #[tokio::test]
    async fn split_persists_change_retires_destination_and_preserves_receipt() {
        let coins = Arc::new(InMemoryCoinRepository::with_coins([Coin {
            exponent: 3,
            derivation_index: 100,
            age: Some(1),
            state: crate::CoinState::Available,
        }]));
        let vouchers = Arc::new(InMemoryVoucherRepository::default());
        let wal = Arc::new(MemWal::default());
        let service = Arc::new(RegularCoinTransferService::new(
            &[0x33; 32],
            RegularCoinTransferParts {
                spawner: crate::test_spawner(),
                coins: Arc::clone(&coins) as Arc<_>,
                vouchers,
                wal: Arc::clone(&wal) as Arc<_>,
                allocator: Arc::new(CoinAllocator::new(Arc::new(
                    InMemoryCoinageIndexStore::default(),
                ))),
                submitter: Arc::new(MockSubmitter {
                    wal: Arc::clone(&wal),
                    fail_unload: HashSet::new(),
                }),
                denominations: context(),
                max_consolidation: 100,
                clock: Arc::new(FixedClock(1_000)),
                session_generation: 4,
                committer: None,
            },
        ));
        let preview = service.preview(3).await.unwrap();
        assert_eq!(preview.strategy, TransferPreviewStrategy::Split);
        service
            .confirm(
                &preview.preview_id,
                TransferPreviewChoice::Full,
                |memo| async move {
                    assert_eq!(memo.total_value, 3);
                    Ok(())
                },
            )
            .await
            .unwrap();
        let rows = coins.list().await.unwrap();
        assert_eq!(
            rows.iter()
                .find(|coin| coin.derivation_index == 100)
                .unwrap()
                .state,
            crate::CoinState::Spent
        );
        assert!(
            rows.iter()
                .any(|coin| coin.state == crate::CoinState::Spent && coin.derivation_index != 100)
        );
        assert!(
            rows.iter()
                .any(|coin| coin.state == crate::CoinState::Available)
        );
        assert_eq!(
            wal.load_operation(&preview.preview_id).await.unwrap()[0].operation,
            WalOperation::TransferCompleted
        );
    }

    #[tokio::test]
    async fn multi_group_unload_commits_success_and_keeps_ambiguous_group_reserved() {
        let (service, _, vouchers, wal) =
            service(vec![voucher(1, 1, 1), voucher(2, 0, 2)], HashSet::from([2]));
        let preview = service.preview(3).await.unwrap();
        service
            .confirm(
                &preview.preview_id,
                TransferPreviewChoice::Full,
                |_| async { Ok(()) },
            )
            .await
            .unwrap();
        let remaining = vouchers.list().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].derivation_index, 2);
        assert_eq!(remaining[0].local_state, VoucherLocalState::PendingTransfer);
        let entries = wal.load_operation(&preview.preview_id).await.unwrap();
        assert!(
            entries
                .iter()
                .any(|entry| entry.operation == WalOperation::TransferAccepted)
        );
        let unresolved = entries
            .iter()
            .find(|entry| entry.operation == WalOperation::IntoCoins)
            .unwrap();
        assert_eq!(unresolved.payload.input_vouchers[0].derivation_index, 2);
        assert!(matches!(
            unresolved.checkpoint,
            CheckpointBlock::Known { .. }
        ));
    }

    #[test]
    fn spendable_voucher_snapshot_never_returns_false_empty_wallet() {
        let inventory = vec![
            voucher(1, 10, 1),
            voucher(2, 4, 2),
            voucher(3, 4, 3),
            voucher(4, 3, 4),
            voucher(5, 1, 5),
            voucher(6, 1, 6),
        ];
        let selector = CoinSelector::new(context(), 100);
        for amount in 1..=1_068 {
            assert_ne!(
                selector.select(amount, &[], &inventory, 1_000),
                Err(CoinSelectionError::EmptyWallet),
                "false empty wallet at amount {amount}"
            );
        }
    }

    /// Property sweep for the opaque boundary: confirmation must execute the
    /// exact amount retained by preview, across every representable amount in
    /// the source voucher. It must never reselect a different total.
    #[tokio::test]
    async fn preview_to_confirm_preserves_every_representable_amount() {
        for amount in 1..=64_u128 {
            let (service, _, _, _) = service(vec![voucher(1, 6, 1)], HashSet::new());
            let preview = service.preview(amount).await.unwrap();
            assert_eq!(preview.full_amount, amount);
            service
                .confirm(
                    &preview.preview_id,
                    TransferPreviewChoice::Full,
                    move |memo| async move {
                        assert_eq!(memo.total_value, amount);
                        Ok(())
                    },
                )
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn preview_rejects_a_changed_repository_snapshot() {
        let inventory = vec![voucher(1, 10, 1)];
        let (service, _coins, vouchers, _) = service(inventory, HashSet::new());
        let preview = service.preview(100).await.unwrap();
        vouchers
            .set_local_state(1, VoucherLocalState::PendingTransfer)
            .await
            .unwrap();
        assert_eq!(
            service
                .confirm(
                    &preview.preview_id,
                    TransferPreviewChoice::Full,
                    |_| async { Ok(()) }
                )
                .await,
            Err(RegularTransferError::BalanceChanged)
        );
    }

    #[tokio::test]
    async fn same_preview_confirmations_are_coalesced() {
        let (service, _, _, _) = service(vec![voucher(1, 10, 1)], HashSet::new());
        let preview = service.preview(100).await.unwrap();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let first = {
            let service = Arc::clone(&service);
            let calls = Arc::clone(&calls);
            let id = preview.preview_id.clone();
            tokio::spawn(async move {
                service
                    .confirm(&id, TransferPreviewChoice::Full, move |_| async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        Ok(())
                    })
                    .await
            })
        };
        let second = service.confirm(
            &preview.preview_id,
            TransferPreviewChoice::Full,
            |_| async { panic!("coalesced confirmation must not receive the memo") },
        );
        assert!(first.await.unwrap().is_ok());
        assert!(second.await.is_ok());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn diagnostics_contain_only_public_selection_state() {
        let fixture = [
            voucher(7, 10, 9),
            voucher(8, 4, 9),
            voucher(9, 4, 9),
            voucher(10, 3, 9),
            voucher(11, 1, 9),
            voucher(12, 1, 9),
        ];
        let rows = voucher_selection_diagnostics(&fixture, 0);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].derivation_index, 7);
        assert_eq!(
            rows[0].remote_state,
            VoucherRemoteState::InRecycler { recycler_index: 9 }
        );
        assert_eq!(rows[0].effective_privacy, EffectivePrivacy::Full);
    }

    fn restart(service: &RegularCoinTransferService) -> Arc<RegularCoinTransferService> {
        Arc::new(RegularCoinTransferService::new(
            &[0x33; 32],
            RegularCoinTransferParts {
                spawner: crate::test_spawner(),
                coins: Arc::clone(&service.coins),
                vouchers: Arc::clone(&service.vouchers),
                wal: Arc::clone(&service.wal),
                allocator: Arc::clone(&service.allocator),
                submitter: Arc::clone(&service.submitter),
                denominations: service.denominations.clone(),
                max_consolidation: service.max_consolidation,
                clock: Arc::clone(&service.clock),
                session_generation: service.session_generation + 1,
                committer: service.committer.clone(),
            },
        ))
    }

    #[tokio::test]
    async fn restart_after_transport_acceptance_replays_same_allocations_once() {
        let (service, coins, vouchers, wal) = service(vec![voucher(1, 3, 1)], HashSet::new());
        let preview = service.preview(3).await.unwrap();
        assert_eq!(preview.max_debit_amount, 8);
        let (accepted, received) = tokio::sync::oneshot::channel();
        let transfer = {
            let service = Arc::clone(&service);
            tokio::spawn(async move {
                service
                    .confirm_operation(
                        "host:payment-1",
                        &preview.preview_id,
                        TransferPreviewChoice::Full,
                        move |memo| async move {
                            accepted.send(memo.identifier()).unwrap();
                            // Transport durably accepted but its acknowledgement
                            // has not returned when the process disappears.
                            std::future::pending::<Result<(), ()>>().await
                        },
                    )
                    .await
            })
        };
        let accepted_identifier = received.await.unwrap();
        let before = wal.load_operation("host:payment-1").await.unwrap();
        assert!(
            before
                .iter()
                .any(|entry| entry.operation == WalOperation::TransferPrepared)
        );
        assert!(
            before
                .iter()
                .any(|entry| entry.operation == WalOperation::IntoCoins)
        );
        assert_eq!(
            vouchers.list().await.unwrap()[0].local_state,
            VoucherLocalState::PendingTransfer
        );
        transfer.abort();
        let _ = transfer.await;
        let restarted = restart(&service);
        restarted
            .resume_operation("host:payment-1", move |memo| async move {
                assert_eq!(memo.identifier(), accepted_identifier);
                assert_eq!(memo.total_value, 3);
                Ok(())
            })
            .await
            .unwrap();
        assert!(vouchers.list().await.unwrap().is_empty());
        let after = coins.list().await.unwrap();
        assert_eq!(
            after
                .iter()
                .filter(|coin| coin.state == CoinState::Available)
                .map(|coin| context().value_in_planks(coin.exponent))
                .sum::<u128>(),
            5
        );
        let receipt = wal.load_operation("host:payment-1").await.unwrap();
        assert_eq!(receipt.len(), 1);
        assert_eq!(receipt[0].operation, WalOperation::TransferCompleted);
        assert_eq!(
            receipt[0].payload.output_coins,
            before
                .iter()
                .find(|entry| entry.operation == WalOperation::TransferPrepared)
                .unwrap()
                .payload
                .output_coins
        );
        restarted
            .confirm_operation(
                "host:payment-1",
                "no-preview-after-restart",
                TransferPreviewChoice::Full,
                |_| async { panic!("completed operation must not hand off or debit again") },
            )
            .await
            .unwrap();
        assert_eq!(coins.list().await.unwrap(), after);
    }

    #[tokio::test]
    async fn rejected_operation_keeps_identity_but_releases_only_its_inputs() {
        let (service, _, vouchers, wal) = service(vec![voucher(1, 3, 1)], HashSet::new());
        let preview = service.preview(3).await.unwrap();
        assert_eq!(
            service
                .confirm_operation(
                    "rejected",
                    &preview.preview_id,
                    TransferPreviewChoice::Full,
                    |_| async { Err(()) }
                )
                .await,
            Err(RegularTransferError::HandoffRejected)
        );
        assert_eq!(
            vouchers.list().await.unwrap()[0].local_state,
            VoucherLocalState::Available
        );
        let restarted = restart(&service);
        assert_eq!(
            restarted
                .resume_operation("rejected", |_| async {
                    panic!("rejected operation identity must not start a new debit")
                })
                .await,
            Err(RegularTransferError::HandoffRejected)
        );
        let entries = wal.load_operation("rejected").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, WalOperation::TransferRejected);
    }

    #[tokio::test]
    async fn exact_operation_preserves_receipt_without_reexporting_coin_secrets() {
        let (service, coins, _, wal) = service(Vec::new(), HashSet::new());
        coins
            .upsert(&Coin {
                derivation_index: 90,
                exponent: 2,
                age: Some(1),
                state: CoinState::Available,
            })
            .await
            .unwrap();
        let preview = service.preview(4).await.unwrap();
        let observed = Arc::clone(&wal);
        service
            .confirm_operation(
                "exact",
                &preview.preview_id,
                TransferPreviewChoice::Full,
                move |memo| async move {
                    let entries = observed.load_operation("exact").await.unwrap();
                    let handoff = entries
                        .iter()
                        .find(|entry| entry.operation == WalOperation::SecretHandoff)
                        .unwrap();
                    assert_eq!(handoff.payload.input_coins[0].derivation_index, 90);
                    assert_eq!(memo.total_value, 4);
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert_eq!(coins.list().await.unwrap()[0].state, CoinState::Spent);
        restart(&service)
            .resume_operation("exact", |_| async {
                panic!("accepted exact-coin secret must not be handed out again")
            })
            .await
            .unwrap();
        assert_eq!(
            wal.load_operation("exact").await.unwrap()[0].operation,
            WalOperation::TransferCompleted
        );
    }
}
