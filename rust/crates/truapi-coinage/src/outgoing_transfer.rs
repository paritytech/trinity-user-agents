// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::collections::HashMap;
use std::future::Future;
use {parking_lot::Mutex, std::sync::Arc};

use futures::future::AbortHandle;
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

use crate::clock::Clock;
use crate::denomination::DenominationBreakdownContext;
use crate::keys::CoinKeypairFactory;
use crate::memo::{MemoEntry, TransferMemo};
use crate::model::{Coin, CoinState};
use crate::query::CoinOnChainQueryService;
use crate::repo::{CoinRepository, TransferContext, VoucherRepository};
use crate::selection::{CoinSelectionError, CoinSelector, TransferStrategy};
use crate::wal::{
    CheckpointBlock, TransferWalEntry, WalCoinRef, WalOperation, WalPayload, WalStore,
};

/// Dependencies that are stable for one active signing session.
pub struct OutgoingCoinTransferParts {
    pub spawner: crate::Spawner,
    pub coins: Arc<dyn CoinRepository>,
    pub vouchers: Arc<dyn VoucherRepository>,
    pub wal: Arc<dyn WalStore>,
    pub on_chain: Arc<CoinOnChainQueryService>,
    pub denominations: DenominationBreakdownContext,
    pub clock: Arc<dyn Clock>,
    /// Finalized heads allowed for the opportunistic live settlement watch.
    /// A timeout retains the durable reservation for a later reconciliation.
    pub settlement_timeout_heads: u32,
}

/// The handoff returned a definitive *not accepted* result.
/// This is deliberately a unit type. Transport diagnostics belong at the
/// adapter boundary; carrying an arbitrary error string through the
/// key-owning service makes accidental secret interpolation much easier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutgoingHandoffRejected;

impl std::fmt::Display for OutgoingHandoffRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("outgoing coin memo was rejected before acceptance")
    }
}

impl std::error::Error for OutgoingHandoffRejected {}

/// Typed outgoing-transfer failures. No variant contains memo bytes or raw
/// coin keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutgoingTransferError {
    Selection(CoinSelectionError),
    /// The amount is representable and funded only by an on-chain split or
    /// voucher unload. This service intentionally performs neither.
    RequiresOnChainPreparation,
    Query(String),
    KeyDerivation(String),
    Reservation(String),
    Journal(String),
    HandoffRejected,
    /// An explicit pre-handoff rejection was received, but the durable
    /// reservation could not be completely removed. Funds remain reserved.
    Rollback(String),
}

impl std::fmt::Display for OutgoingTransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selection(error) => write!(formatter, "{error}"),
            Self::RequiresOnChainPreparation => formatter
                .write_str("exact whole coins are unavailable; on-chain preparation is required"),
            Self::Query(error) => write!(formatter, "coin query failed: {error}"),
            Self::KeyDerivation(error) => write!(formatter, "coin key derivation failed: {error}"),
            Self::Reservation(error) => write!(formatter, "coin reservation failed: {error}"),
            Self::Journal(error) => write!(formatter, "coin transfer journal failed: {error}"),
            Self::HandoffRejected => {
                formatter.write_str("outgoing coin memo was rejected before acceptance")
            }
            Self::Rollback(error) => {
                write!(
                    formatter,
                    "outgoing coin reservation rollback failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for OutgoingTransferError {}

impl From<CoinSelectionError> for OutgoingTransferError {
    fn from(error: CoinSelectionError) -> Self {
        Self::Selection(error)
    }
}

/// Outcome of one startup/manual pending-transfer reconciliation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutgoingTransferReconciliation {
    pub confirmed_spent: Vec<u32>,
    pub still_pending: Vec<u32>,
    pub completed_journals: usize,
}

/// Outgoing transfer engine tied to exactly one root entropy.
/// Construct a fresh instance on signing-session activation and drop (or
/// [`shutdown`](Self::shutdown)) it on session teardown.
pub struct OutgoingCoinTransferService {
    spawner: crate::Spawner,
    key_factory: Arc<CoinKeypairFactory>,
    coins: Arc<dyn CoinRepository>,
    vouchers: Arc<dyn VoucherRepository>,
    wal: Arc<dyn WalStore>,
    on_chain: Arc<CoinOnChainQueryService>,
    denominations: DenominationBreakdownContext,
    clock: Arc<dyn Clock>,
    settlement_timeout_heads: u32,
    handoff_lock: AsyncMutex<()>,
    settlement_tasks: Mutex<Vec<AbortHandle>>,
}

impl OutgoingCoinTransferService {
    pub fn new(root_entropy: &[u8], parts: OutgoingCoinTransferParts) -> Self {
        Self {
            key_factory: Arc::new(CoinKeypairFactory::new(root_entropy)),
            coins: parts.coins,
            vouchers: parts.vouchers,
            wal: parts.wal,
            on_chain: parts.on_chain,
            denominations: parts.denominations,
            clock: parts.clock,
            settlement_timeout_heads: parts.settlement_timeout_heads.max(1),
            handoff_lock: AsyncMutex::new(()),
            settlement_tasks: Mutex::new(Vec::new()),
            spawner: parts.spawner,
        }
    }

    /// Converts a W3S/UI CASH-cent amount to the raw planks required by the
    /// exact-coin selector.
    pub fn cash_cents_to_planks(&self, cents: u128) -> Option<u128> {
        self.denominations.cash_cents_to_planks(cents)
    }

    pub fn planks_per_cash_cent(&self) -> u128 {
        self.denominations.asset_unit
    }

    /// Selects and hands off exact whole coins.
    /// `handoff` owns the memo and must return `Err` **only** when it can
    /// certify that no recipient/transport durably accepted the secret. If
    /// acceptance is ambiguous (timeout, cancellation after submit, lost
    /// acknowledgement), it must return `Ok`; the normal payment/chat
    /// tracker can report the transport uncertainty while the coin
    /// reservation remains safe.
    ///
    /// For a host-correlated payment with durable preparation/restart
    /// receipts, use `RegularCoinTransferService::confirm_operation`, which
    /// also handles exact whole-coin transfers without chain preparation.
    pub async fn handoff_exact<F, Fut>(
        &self,
        amount_planks: u128,
        handoff: F,
    ) -> Result<(), OutgoingTransferError>
    where
        F: FnOnce(TransferMemo) -> Fut,
        Fut: Future<Output = Result<(), OutgoingHandoffRejected>>,
    {
        // Selection + reservation is serialized per session so two callers
        // cannot observe the same pre-reservation snapshot.
        let _guard = self.handoff_lock.lock().await;
        let present = self.present_local_coins().await?;
        let selection = CoinSelector::new(self.denominations.clone(), usize::MAX).select(
            amount_planks,
            &present,
            &[],
            self.clock.now_ms(),
        )?;
        let selected = match selection.strategy {
            TransferStrategy::ExactMatch { coins } => coins,
            TransferStrategy::Split { .. } | TransferStrategy::UnloadIntoCoins { .. } => {
                return Err(OutgoingTransferError::RequiresOnChainPreparation);
            }
        };

        let indices = selected
            .iter()
            .map(|coin| coin.derivation_index)
            .collect::<Vec<_>>();
        let public_keys = indices
            .iter()
            .map(|index| {
                self.key_factory
                    .public_key(*index)
                    .map_err(OutgoingTransferError::KeyDerivation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let entries = indices
            .iter()
            .map(|index| {
                self.key_factory
                    .secret_bytes(*index)
                    .map(MemoEntry)
                    .map_err(OutgoingTransferError::KeyDerivation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let memo = TransferMemo {
            entries,
            total_value: amount_planks,
        };
        let memo_identifier = memo.identifier();
        let entry_id = format!("secret-handoff-{}", hex::encode(memo_identifier));
        let journal = TransferWalEntry {
            entry_id: entry_id.clone(),
            operation: WalOperation::SecretHandoff,
            payload: WalPayload {
                input_coins: selected
                    .iter()
                    .map(|coin| WalCoinRef {
                        derivation_index: coin.derivation_index,
                        exponent: coin.exponent,
                    })
                    .collect(),
                ..WalPayload::default()
            },
            // Secret handoffs ignore extrinsic mortality. Pending remains
            // the truthful checkpoint because no extrinsic is broadcast.
            checkpoint: CheckpointBlock::Pending,
            created_at_ms: self.clock.now_ms(),
        };

        let context = Arc::new(TransferContext::new(
            Arc::clone(&self.coins),
            Arc::clone(&self.vouchers),
        ));
        if let Err(error) = context.reserve(&indices, &[]).await {
            let rollback = context.revert().await;
            return Err(match rollback {
                Ok(()) => OutgoingTransferError::Reservation(error),
                Err(rollback) => OutgoingTransferError::Rollback(format!(
                    "reserve error: {error}; state restore error: {rollback}"
                )),
            });
        }
        if let Err(error) = self.wal.save(&journal).await {
            // A local persistence failure is expected to mean no row, but
            // delete the deterministic id first to close an ambiguous
            // commit edge before making the inputs selectable again.
            if let Err(cleanup) = self.wal.delete(&entry_id).await {
                return Err(OutgoingTransferError::Rollback(format!(
                    "journal error: {error}; journal cleanup error: {cleanup}; inputs retained"
                )));
            }
            return match context.revert().await {
                Ok(()) => Err(OutgoingTransferError::Journal(error)),
                Err(rollback) => Err(OutgoingTransferError::Rollback(format!(
                    "journal error: {error}; state restore error: {rollback}"
                ))),
            };
        }

        if handoff(memo).await.is_err() {
            // Delete protection before restoring availability. The opposite
            // order leaves a crash window with an Available coin still
            // referenced by an indefinite secret-handoff journal.
            if let Err(error) = self.wal.delete(&entry_id).await {
                return Err(OutgoingTransferError::Rollback(format!(
                    "handoff rejected; journal cleanup error: {error}; inputs retained"
                )));
            }
            if let Err(error) = context.revert().await {
                return Err(OutgoingTransferError::Rollback(format!(
                    "handoff rejected; state restore error: {error}"
                )));
            }
            return Err(OutgoingTransferError::HandoffRejected);
        }

        self.spawn_settlement_watch(context, indices, public_keys, entry_id);
        Ok(())
    }

    /// Reconciles every durable secret-handoff entry in one ordered batch.
    /// Absent inputs become `Spent`; present inputs remain (or are restored
    /// to) `PendingTransfer`. A journal is removed only when all its inputs
    /// are absent.
    pub async fn reconcile_pending_transfers(
        &self,
    ) -> Result<OutgoingTransferReconciliation, OutgoingTransferError> {
        let journals = self
            .wal
            .load_all()
            .await
            .map_err(OutgoingTransferError::Journal)?
            .into_iter()
            // Correlated operations have a durable parent and are reconciled
            // by TransferRecoveryService. In particular, Prepared must not be
            // mistaken for a proven accepted handoff here.
            .filter(|entry| {
                entry.operation == WalOperation::SecretHandoff
                    && entry.operation_parent_id().is_none()
            })
            .collect::<Vec<_>>();
        if journals.is_empty() {
            return Ok(OutgoingTransferReconciliation::default());
        }

        let mut references = Vec::new();
        for (journal_index, journal) in journals.iter().enumerate() {
            for input in &journal.payload.input_coins {
                let public_key = self
                    .key_factory
                    .public_key(input.derivation_index)
                    .map_err(OutgoingTransferError::KeyDerivation)?;
                references.push((journal_index, input.derivation_index, public_key));
            }
        }
        let public_keys = references
            .iter()
            .map(|(_, _, key)| *key)
            .collect::<Vec<_>>();
        let remote = self
            .on_chain
            .fetch_coins(&public_keys, None)
            .await
            .map_err(|error| OutgoingTransferError::Query(error.to_string()))?;
        if remote.len() != references.len() {
            return Err(OutgoingTransferError::Query(
                "coin query returned an incomplete batch".into(),
            ));
        }
        let local_states = self
            .coins
            .list()
            .await
            .map_err(OutgoingTransferError::Reservation)?
            .into_iter()
            .map(|coin| (coin.derivation_index, coin.state))
            .collect::<HashMap<_, _>>();

        let mut report = OutgoingTransferReconciliation::default();
        let mut journal_has_present = vec![false; journals.len()];
        for ((journal_index, derivation_index, _), reading) in references.into_iter().zip(remote) {
            if reading.is_none() {
                if local_states.get(&derivation_index) != Some(&CoinState::Spent) {
                    self.coins
                        .set_state(derivation_index, CoinState::Spent)
                        .await
                        .map_err(OutgoingTransferError::Reservation)?;
                }
                report.confirmed_spent.push(derivation_index);
            } else {
                journal_has_present[journal_index] = true;
                if local_states.get(&derivation_index) == Some(&CoinState::Available) {
                    self.coins
                        .set_state(derivation_index, CoinState::PendingTransfer)
                        .await
                        .map_err(OutgoingTransferError::Reservation)?;
                }
                report.still_pending.push(derivation_index);
            }
        }
        for (journal, has_present) in journals.iter().zip(journal_has_present) {
            if !has_present && !journal.payload.input_coins.is_empty() {
                self.wal
                    .delete(&journal.entry_id)
                    .await
                    .map_err(OutgoingTransferError::Journal)?;
                report.completed_journals += 1;
            }
        }
        report.confirmed_spent.sort_unstable();
        report.confirmed_spent.dedup();
        report.still_pending.sort_unstable();
        report.still_pending.dedup();
        Ok(report)
    }

    /// Aborts live finalized-head waits. Durable journal rows and
    /// reservations deliberately remain for the next session startup.
    pub fn shutdown(&self) {
        for task in self.settlement_tasks.lock().drain(..) {
            task.abort();
        }
    }

    async fn present_local_coins(&self) -> Result<Vec<Coin>, OutgoingTransferError> {
        let local = self
            .coins
            .list()
            .await
            .map_err(OutgoingTransferError::Reservation)?;
        let candidates = local
            .into_iter()
            .filter(|coin| coin.is_selectable())
            .collect::<Vec<_>>();
        let public_keys = candidates
            .iter()
            .map(|coin| {
                self.key_factory
                    .public_key(coin.derivation_index)
                    .map_err(OutgoingTransferError::KeyDerivation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let remote = self
            .on_chain
            .fetch_coins(&public_keys, None)
            .await
            .map_err(|error| OutgoingTransferError::Query(error.to_string()))?;
        if remote.len() != candidates.len() {
            return Err(OutgoingTransferError::Query(
                "coin query returned an incomplete batch".into(),
            ));
        }
        Ok(candidates
            .into_iter()
            .zip(remote)
            .filter_map(|(mut local, remote)| {
                let remote = remote?;
                local.exponent = remote.exponent;
                local.age = Some(remote.age);
                local.is_selectable().then_some(local)
            })
            .collect())
    }

    fn spawn_settlement_watch(
        &self,
        context: Arc<TransferContext>,
        indices: Vec<u32>,
        public_keys: Vec<[u8; 32]>,
        entry_id: String,
    ) {
        let on_chain = Arc::clone(&self.on_chain);
        let wal = Arc::clone(&self.wal);
        let timeout = self.settlement_timeout_heads;
        let task = crate::tasks::spawn_abortable(&self.spawner, async move {
            match on_chain
                .await_all_coins_off_chain(&public_keys, timeout)
                .await
            {
                Ok(()) => {
                    if let Err(error) = context.process(&indices, &[]).await {
                        warn!(
                            %error,
                            coin_count = indices.len(),
                            "outgoing transfer settlement could not persist spent inputs"
                        );
                        return;
                    }
                    if let Err(error) = wal.delete(&entry_id).await {
                        warn!(
                            %error,
                            coin_count = indices.len(),
                            "outgoing transfer settlement could not clear its journal"
                        );
                    }
                }
                Err(error) => {
                    // Never revert after handoff. Startup/manual recovery
                    // can retry with a fresh connection.
                    warn!(
                        %error,
                        coin_count = indices.len(),
                        "outgoing transfer settlement watch ended; reservation retained"
                    );
                }
            }
        });
        self.settlement_tasks.lock().push(task);
    }
}

impl Drop for OutgoingCoinTransferService {
    fn drop(&mut self) {
        for task in self.settlement_tasks.get_mut().drain(..) {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use futures::StreamExt;
    use futures::channel::mpsc;
    use futures::stream::{self, BoxStream};
    use parity_scale_codec::Encode;

    use super::*;
    use crate::clock::FixedClock;
    use crate::query::{CoinageStorageKey, CoinageStorageQuery};
    use crate::repo::{InMemoryCoinRepository, InMemoryVoucherRepository};

    const ENTROPY: [u8; 16] = [0x17; 16];
    type HeadReceiver = mpsc::UnboundedReceiver<Result<[u8; 32], String>>;

    #[derive(Default)]
    struct MemWal {
        entries: Mutex<Vec<TransferWalEntry>>,
    }

    #[async_trait]
    impl WalStore for MemWal {
        async fn save(&self, entry: &TransferWalEntry) -> Result<(), String> {
            let mut entries = self.entries.lock();
            if entries.iter().any(|saved| saved.entry_id == entry.entry_id) {
                return Err("duplicate journal".into());
            }
            entries.push(entry.clone());
            Ok(())
        }

        async fn save_all(&self, entries: &[TransferWalEntry]) -> Result<(), String> {
            for entry in entries {
                self.save(entry).await?;
            }
            Ok(())
        }

        async fn update_checkpoint(
            &self,
            entry_id: &str,
            checkpoint: CheckpointBlock,
        ) -> Result<(), String> {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|entry| entry.entry_id == entry_id)
                .ok_or_else(|| "journal not found".to_string())?;
            entry.checkpoint = checkpoint;
            Ok(())
        }

        async fn load_all(&self) -> Result<Vec<TransferWalEntry>, String> {
            Ok(self.entries.lock().clone())
        }

        async fn delete(&self, entry_id: &str) -> Result<(), String> {
            self.entries
                .lock()
                .retain(|entry| entry.entry_id != entry_id);
            Ok(())
        }
    }

    struct TestStorage {
        values: Mutex<HashMap<CoinageStorageKey, Vec<u8>>>,
        heads: Mutex<Option<HeadReceiver>>,
    }

    impl Default for TestStorage {
        fn default() -> Self {
            Self {
                values: Mutex::new(HashMap::new()),
                heads: Mutex::new(None),
            }
        }
    }

    impl TestStorage {
        fn set_coin(&self, public_key: [u8; 32], exponent: i8, age: u16) {
            self.values.lock().insert(
                CoinageStorageKey::Coin(public_key),
                (exponent, age).encode(),
            );
        }

        fn remove_coin(&self, public_key: [u8; 32]) {
            self.values
                .lock()
                .remove(&CoinageStorageKey::Coin(public_key));
        }

        fn head_channel(&self) -> mpsc::UnboundedSender<Result<[u8; 32], String>> {
            let (sender, receiver) = mpsc::unbounded();
            *self.heads.lock() = Some(receiver);
            sender
        }
    }

    #[async_trait]
    impl CoinageStorageQuery for TestStorage {
        async fn query(
            &self,
            keys: &[CoinageStorageKey],
            _at: Option<[u8; 32]>,
        ) -> Result<Vec<Option<Vec<u8>>>, String> {
            let values = self.values.lock();
            Ok(keys.iter().map(|key| values.get(key).cloned()).collect())
        }

        fn finalized_heads(&self) -> BoxStream<'static, Result<[u8; 32], String>> {
            self.heads
                .lock()
                .take()
                .map(StreamExt::boxed)
                .unwrap_or_else(|| stream::pending().boxed())
        }
    }

    struct Harness {
        service: OutgoingCoinTransferService,
        coins: Arc<InMemoryCoinRepository>,
        wal: Arc<MemWal>,
        storage: Arc<TestStorage>,
    }

    fn coin(index: u32) -> Coin {
        Coin {
            // Deliberately stale: selection must use the on-chain value.
            exponent: 0,
            derivation_index: index,
            age: None,
            state: CoinState::Available,
        }
    }

    fn denominations() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 6,
            min_exponent: 0,
            precision: 2,
        }
    }

    fn harness(local: Vec<Coin>) -> Harness {
        let coins = Arc::new(InMemoryCoinRepository::with_coins(local));
        let wal = Arc::new(MemWal::default());
        let storage = Arc::new(TestStorage::default());
        let service = OutgoingCoinTransferService::new(
            &ENTROPY,
            OutgoingCoinTransferParts {
                spawner: crate::test_spawner(),
                coins: Arc::clone(&coins) as Arc<_>,
                vouchers: Arc::new(InMemoryVoucherRepository::default()),
                wal: Arc::clone(&wal) as Arc<_>,
                on_chain: Arc::new(CoinOnChainQueryService::new(Arc::clone(&storage) as Arc<_>)),
                denominations: denominations(),
                clock: Arc::new(FixedClock(1_234)),
                settlement_timeout_heads: 3,
            },
        );
        Harness {
            service,
            coins,
            wal,
            storage,
        }
    }

    async fn state(coins: &InMemoryCoinRepository, index: u32) -> CoinState {
        coins
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|coin| coin.derivation_index == index)
            .unwrap()
            .state
    }

    #[tokio::test]
    async fn exact_on_chain_selection_reserves_before_handoff_and_settles_on_absence() {
        let harness = harness(vec![coin(1), coin(2), coin(3)]);
        let keys = CoinKeypairFactory::new(&ENTROPY);
        harness.storage.set_coin(keys.public_key(1).unwrap(), 2, 1); // 40
        harness.storage.set_coin(keys.public_key(2).unwrap(), 1, 1); // 20
        harness.storage.set_coin(keys.public_key(3).unwrap(), 0, 1); // 10
        let heads = harness.storage.head_channel();
        let observed = Arc::new(Mutex::new(None));

        harness
            .service
            .handoff_exact(60, {
                let observed = Arc::clone(&observed);
                let coins = Arc::clone(&harness.coins);
                move |memo| async move {
                    assert_eq!(state(&coins, 1).await, CoinState::PendingTransfer);
                    assert_eq!(state(&coins, 2).await, CoinState::PendingTransfer);
                    assert_eq!(state(&coins, 3).await, CoinState::Available);
                    *observed.lock() = Some((
                        memo.total_value,
                        memo.entries.iter().map(|entry| entry.0).collect::<Vec<_>>(),
                    ));
                    Ok(())
                }
            })
            .await
            .unwrap();

        let observed = observed.lock().take().unwrap();
        assert_eq!(observed.0, 60);
        assert_eq!(
            observed.1,
            vec![keys.secret_bytes(1).unwrap(), keys.secret_bytes(2).unwrap()]
        );
        let journal = harness.wal.load_all().await.unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0].operation, WalOperation::SecretHandoff);
        assert_eq!(journal[0].created_at_ms, 1_234);

        harness.storage.remove_coin(keys.public_key(1).unwrap());
        harness.storage.remove_coin(keys.public_key(2).unwrap());
        heads.unbounded_send(Ok([9; 32])).unwrap();
        for _ in 0..40 {
            if state(&harness.coins, 1).await == CoinState::Spent {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(state(&harness.coins, 1).await, CoinState::Spent);
        assert_eq!(state(&harness.coins, 2).await, CoinState::Spent);
        assert!(harness.wal.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_insufficient_nonrepresentable_and_preparation_only_amounts() {
        let harness = harness(vec![coin(1)]);
        let keys = CoinKeypairFactory::new(&ENTROPY);
        harness.storage.set_coin(keys.public_key(1).unwrap(), 2, 1); // 40

        let never = |_memo| async { Ok::<(), OutgoingHandoffRejected>(()) };
        assert_eq!(
            harness.service.handoff_exact(5, never).await,
            Err(OutgoingTransferError::Selection(
                CoinSelectionError::AmountNotRepresentable { remainder: 5 }
            ))
        );
        assert_eq!(
            harness
                .service
                .handoff_exact(80, |_memo| async { Ok(()) })
                .await,
            Err(OutgoingTransferError::Selection(
                CoinSelectionError::InsufficientFunds
            ))
        );
        assert_eq!(
            harness
                .service
                .handoff_exact(20, |_memo| async { Ok(()) })
                .await,
            Err(OutgoingTransferError::RequiresOnChainPreparation)
        );
        assert_eq!(state(&harness.coins, 1).await, CoinState::Available);
        assert!(harness.wal.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn explicit_pre_handoff_rejection_removes_journal_then_rolls_back() {
        let harness = harness(vec![coin(4)]);
        let keys = CoinKeypairFactory::new(&ENTROPY);
        harness.storage.set_coin(keys.public_key(4).unwrap(), 1, 1);
        let saw_commit_boundary = Arc::new(Mutex::new(false));

        let result = harness
            .service
            .handoff_exact(20, {
                let saw_commit_boundary = Arc::clone(&saw_commit_boundary);
                let coins = Arc::clone(&harness.coins);
                let wal = Arc::clone(&harness.wal);
                move |_memo| async move {
                    assert_eq!(state(&coins, 4).await, CoinState::PendingTransfer);
                    assert_eq!(wal.load_all().await.unwrap().len(), 1);
                    *saw_commit_boundary.lock() = true;
                    Err(OutgoingHandoffRejected)
                }
            })
            .await;

        assert_eq!(result, Err(OutgoingTransferError::HandoffRejected));
        assert!(*saw_commit_boundary.lock());
        assert_eq!(state(&harness.coins, 4).await, CoinState::Available);
        assert!(harness.wal.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accepted_handoff_stays_pending_and_cannot_be_selected_twice() {
        let harness = harness(vec![coin(5)]);
        let keys = CoinKeypairFactory::new(&ENTROPY);
        harness.storage.set_coin(keys.public_key(5).unwrap(), 2, 1);
        harness
            .service
            .handoff_exact(40, |_memo| async { Ok(()) })
            .await
            .unwrap();

        assert_eq!(state(&harness.coins, 5).await, CoinState::PendingTransfer);
        assert_eq!(harness.wal.load_all().await.unwrap().len(), 1);
        assert_eq!(
            harness
                .service
                .handoff_exact(40, |_memo| async { Ok(()) })
                .await,
            Err(OutgoingTransferError::Selection(
                CoinSelectionError::EmptyWallet
            ))
        );
        let report = harness.service.reconcile_pending_transfers().await.unwrap();
        assert_eq!(report.still_pending, [5]);
        assert!(report.confirmed_spent.is_empty());
        assert_eq!(harness.wal.load_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn startup_reconciliation_retires_absent_handoffs() {
        let harness = harness(vec![coin(6)]);
        let keys = CoinKeypairFactory::new(&ENTROPY);
        let public_key = keys.public_key(6).unwrap();
        harness.storage.set_coin(public_key, 0, 1);
        harness
            .service
            .handoff_exact(10, |_memo| async { Ok(()) })
            .await
            .unwrap();
        harness.service.shutdown();
        harness.storage.remove_coin(public_key);

        let report = harness.service.reconcile_pending_transfers().await.unwrap();
        assert_eq!(report.confirmed_spent, [6]);
        assert!(report.still_pending.is_empty());
        assert_eq!(report.completed_journals, 1);
        assert_eq!(state(&harness.coins, 6).await, CoinState::Spent);
        assert!(harness.wal.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn diagnostics_never_render_expanded_coin_secrets() {
        let harness = harness(vec![coin(7)]);
        let keys = CoinKeypairFactory::new(&ENTROPY);
        harness.storage.set_coin(keys.public_key(7).unwrap(), 0, 1);
        let secret_hex = hex::encode(keys.secret_bytes(7).unwrap());
        let rendered_memo = Arc::new(Mutex::new(String::new()));

        let error = harness
            .service
            .handoff_exact(10, {
                let rendered_memo = Arc::clone(&rendered_memo);
                move |memo| async move {
                    *rendered_memo.lock() = format!("{memo:?}");
                    Err(OutgoingHandoffRejected)
                }
            })
            .await
            .unwrap_err();
        let diagnostics = format!("{error:?} {error} {}", rendered_memo.lock());
        assert!(!diagnostics.contains(&secret_hex));
    }

    struct FailingReservationRepository {
        inner: Arc<InMemoryCoinRepository>,
        fail_index: u32,
    }

    #[async_trait]
    impl CoinRepository for FailingReservationRepository {
        async fn list(&self) -> Result<Vec<Coin>, String> {
            self.inner.list().await
        }

        async fn upsert(&self, coin: &Coin) -> Result<(), String> {
            self.inner.upsert(coin).await
        }

        async fn set_state(&self, derivation_index: u32, state: CoinState) -> Result<(), String> {
            if derivation_index == self.fail_index && state == CoinState::PendingTransfer {
                return Err("injected reservation failure".into());
            }
            self.inner.set_state(derivation_index, state).await
        }

        async fn remove(&self, derivation_index: u32) -> Result<(), String> {
            self.inner.remove(derivation_index).await
        }
    }

    #[tokio::test]
    async fn partial_reservation_failure_restores_every_prior_input() {
        let inner = Arc::new(InMemoryCoinRepository::with_coins([coin(1), coin(2)]));
        let coins = Arc::new(FailingReservationRepository {
            inner: Arc::clone(&inner),
            fail_index: 2,
        });
        let storage = Arc::new(TestStorage::default());
        let keys = CoinKeypairFactory::new(&ENTROPY);
        storage.set_coin(keys.public_key(1).unwrap(), 1, 1); // 20
        storage.set_coin(keys.public_key(2).unwrap(), 0, 1); // 10
        let wal = Arc::new(MemWal::default());
        let service = OutgoingCoinTransferService::new(
            &ENTROPY,
            OutgoingCoinTransferParts {
                spawner: crate::test_spawner(),
                coins,
                vouchers: Arc::new(InMemoryVoucherRepository::default()),
                wal: Arc::clone(&wal) as Arc<_>,
                on_chain: Arc::new(CoinOnChainQueryService::new(storage)),
                denominations: denominations(),
                clock: Arc::new(FixedClock(0)),
                settlement_timeout_heads: 1,
            },
        );

        let error = service
            .handoff_exact(30, |_memo| async { Ok(()) })
            .await
            .unwrap_err();
        assert!(matches!(error, OutgoingTransferError::Reservation(_)));
        assert_eq!(state(&inner, 1).await, CoinState::Available);
        assert_eq!(state(&inner, 2).await, CoinState::Available);
        assert!(wal.load_all().await.unwrap().is_empty());
    }
}
