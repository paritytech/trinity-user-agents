// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;

use crate::model::{
    Coin, CoinState, Voucher, VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState,
};
use crate::repo::{CoinRepository, VoucherRepository};
use crate::wal::{TransferWalEntry, WalOperation, WalStore};

/// The chain-lookup effect. Presence answers are positional (same order
/// as the input indices); implementations typically batch these through
/// `chain::get_head_storage`.
#[async_trait]
pub trait RecoveryChainProbe: Send + Sync {
    /// Current finalized head number.
    async fn finalized_block(&self) -> Result<u64, String>;

    /// Canonical block hash at a height; `None` when unavailable.
    async fn canonical_hash(&self, block_number: u64) -> Result<Option<[u8; 32]>, String>;

    /// Whether each coin derivation index currently has an on-chain
    /// `CoinsByOwner` entry.
    async fn coins_present(&self, derivation_indices: &[u32]) -> Result<Vec<bool>, String>;

    /// Exact denomination at the finalized snapshot of this recovery sweep.
    /// Responses are positional and must cover every requested index. `None`
    /// proves absence; query errors or truncated batches are not absence.
    async fn coin_exponents(&self, derivation_indices: &[u32]) -> Result<Vec<Option<i16>>, String>;

    /// Whether each voucher derivation index is currently present
    /// on-chain (in a recycler / member set).
    async fn vouchers_present(&self, derivation_indices: &[u32]) -> Result<Vec<bool>, String>;
}

/// One sweep's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    /// Entries whose extrinsic landed (outputs materialized).
    pub landed: usize,
    /// Entries resolved by input consumption without visible outputs.
    pub inputs_consumed: usize,
    /// Entries dead (expired / forked / never broadcast) — inputs
    /// reverted.
    pub reverted: usize,
    /// Entries left journaled for the next sweep.
    pub still_pending: usize,
    pub orphans_restored: usize,
    pub orphans_deleted: usize,
}

pub struct TransferRecoveryService {
    wal: Arc<dyn WalStore>,
    coins: Arc<dyn CoinRepository>,
    vouchers: Arc<dyn VoucherRepository>,
    probe: Arc<dyn RecoveryChainProbe>,
}

impl TransferRecoveryService {
    pub fn new(
        wal: Arc<dyn WalStore>,
        coins: Arc<dyn CoinRepository>,
        vouchers: Arc<dyn VoucherRepository>,
        probe: Arc<dyn RecoveryChainProbe>,
    ) -> Self {
        Self {
            wal,
            coins,
            vouchers,
            probe,
        }
    }

    /// One recovery pass: resolve every WAL entry, then sweep orphans.
    /// Never guesses a terminal state for money in flight — an entry
    /// stays pending until the chain proves it landed or died.
    pub async fn recover(&self) -> Result<RecoveryReport, String> {
        let entries = self.wal.load_all().await?;
        let finalized = self.probe.finalized_block().await?;
        let mut report = RecoveryReport::default();
        let parents = entries
            .iter()
            .filter(|entry| entry.operation.is_transfer_receipt())
            .map(|entry| (entry.entry_id.as_str(), entry.operation))
            .collect::<HashMap<_, _>>();

        for entry in &entries {
            let parent = entry.operation_parent_id();
            let parent_state = parent.as_deref().and_then(|id| parents.get(id)).copied();
            match self.resolve_entry(entry, finalized, parent_state).await {
                Ok(Resolution::Landed) => report.landed += 1,
                Ok(Resolution::InputsConsumed) => report.inputs_consumed += 1,
                Ok(Resolution::Reverted) => report.reverted += 1,
                Ok(Resolution::StillPending) => report.still_pending += 1,
                Ok(Resolution::Receipt) => {}
                Err(error) => {
                    warn!(
                        error,
                        entry_id = entry.entry_id,
                        "wal recovery probe failed"
                    );
                    report.still_pending += 1;
                }
            }
        }

        let remaining = self.wal.load_all().await?;
        for parent in remaining
            .iter()
            .filter(|entry| entry.operation == WalOperation::TransferAccepted)
        {
            if !remaining.iter().any(|entry| {
                entry.entry_id != parent.entry_id
                    && entry.operation_parent_id().as_deref() == Some(parent.entry_id.as_str())
            }) {
                let mut completed = parent.clone();
                completed.operation = WalOperation::TransferCompleted;
                self.wal.save(&completed).await?;
            }
        }
        let (protected_coins, protected_vouchers) = referenced_indices(&remaining);

        for coin in self.coins.list().await? {
            let orphaned = matches!(
                coin.state,
                CoinState::PendingTransfer | CoinState::Recycling
            ) && !protected_coins.contains(&coin.derivation_index);
            if orphaned {
                self.coins
                    .set_state(coin.derivation_index, CoinState::Available)
                    .await?;
                report.orphans_restored += 1;
            }
        }
        for voucher in self.vouchers.list().await? {
            if protected_vouchers.contains(&voucher.derivation_index) {
                continue;
            }
            match voucher.local_state {
                VoucherLocalState::PendingTransfer => {
                    self.vouchers
                        .set_local_state(voucher.derivation_index, VoucherLocalState::Available)
                        .await?;
                    report.orphans_restored += 1;
                }
                VoucherLocalState::PendingOnboarding => {
                    self.vouchers.remove(voucher.derivation_index).await?;
                    report.orphans_deleted += 1;
                }
                _ => {}
            }
        }
        Ok(report)
    }

    async fn resolve_entry(
        &self,
        entry: &TransferWalEntry,
        finalized: u64,
        parent_state: Option<WalOperation>,
    ) -> Result<Resolution, String> {
        if entry.operation.is_transfer_receipt() {
            // Receipts preserve idempotency after children disappear. Prepared
            // is deliberately not interpreted as accepted or rejected.
            return Ok(Resolution::Receipt);
        }
        let correlated = entry.operation_parent_id().is_some();
        if correlated {
            match parent_state {
                Some(WalOperation::TransferRejected)
                    if matches!(entry.checkpoint, crate::wal::CheckpointBlock::Pending) =>
                {
                    self.revert_inputs(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::Reverted);
                }
                Some(WalOperation::TransferAccepted) => {}
                // Missing/ambiguous parent evidence must never unlock inputs.
                _ => return Ok(Resolution::StillPending),
            }
        }
        let dead = match entry.checkpoint {
            crate::wal::CheckpointBlock::Pending => true,
            crate::wal::CheckpointBlock::Known { number, .. } => {
                let canonical = self.probe.canonical_hash(number).await?;
                entry.is_forked(canonical.as_ref()) || entry.is_expired(finalized)
            }
        };

        match entry.operation {
            // A handed-off expanded secret never expires. The recipient
            // may claim it long after an extrinsic mortality window, so
            // presence always remains reserved and only all-input absence
            // is terminal.
            WalOperation::SecretHandoff => {
                if correlated || self.inputs_consumed(entry).await? {
                    self.retire_inputs(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::InputsConsumed);
                }
                Ok(Resolution::StillPending)
            }
            WalOperation::IntoCoins | WalOperation::Split => {
                if !matches!(entry.checkpoint, crate::wal::CheckpointBlock::Pending) {
                    let output_indices: Vec<u32> = entry
                        .payload
                        .output_coins
                        .iter()
                        .map(|c| c.derivation_index)
                        .collect();
                    let outputs = if correlated {
                        let exponents = self.probe.coin_exponents(&output_indices).await?;
                        if exponents.len() != output_indices.len() {
                            return Err("coin recovery probe returned an incomplete batch".into());
                        }
                        if exponents.iter().zip(&entry.payload.output_coins).any(
                            |(actual, expected)| {
                                actual.is_some_and(|exponent| exponent != expected.exponent)
                            },
                        ) {
                            return Err(
                                "recovered output denomination differs from the approved plan"
                                    .into(),
                            );
                        }
                        exponents
                            .into_iter()
                            .map(|exponent| exponent.is_some())
                            .collect::<Vec<_>>()
                    } else {
                        self.probe.coins_present(&output_indices).await?
                    };
                    if outputs.len() != output_indices.len() {
                        return Err("coin recovery probe returned an incomplete batch".into());
                    }
                    let landed = if correlated {
                        !outputs.is_empty() && outputs.iter().all(|present| *present)
                    } else {
                        outputs.iter().any(|present| *present)
                    };
                    if correlated
                        && outputs.iter().any(|present| *present)
                        && (!landed || !self.inputs_consumed(entry).await?)
                    {
                        // Partial/mixed evidence cannot certify this allocation
                        // and must never authorize rebroadcast after mortality.
                        return Ok(Resolution::StillPending);
                    }
                    if landed {
                        // Landed: materialize outputs, retire inputs.
                        for (reference, present) in entry.payload.output_coins.iter().zip(&outputs)
                        {
                            if *present {
                                let destination =
                                    entry.payload.destination_coins.iter().any(|coin| {
                                        coin.derivation_index == reference.derivation_index
                                    });
                                self.coins
                                    .upsert(&Coin {
                                        exponent: reference.exponent,
                                        derivation_index: reference.derivation_index,
                                        age: None,
                                        state: if destination {
                                            CoinState::Spent
                                        } else {
                                            CoinState::Available
                                        },
                                    })
                                    .await?;
                            }
                        }
                        self.retire_transfer_inputs(entry).await?;
                        self.wal.delete(&entry.entry_id).await?;
                        return Ok(Resolution::Landed);
                    }
                    if self.inputs_consumed(entry).await? {
                        if correlated {
                            // Consumption alone does not prove this operation
                            // produced its promised outputs. Retain the receipt
                            // and checkpoint rather than reporting completion.
                            return Ok(Resolution::StillPending);
                        }
                        self.retire_transfer_inputs(entry).await?;
                        self.wal.delete(&entry.entry_id).await?;
                        return Ok(Resolution::InputsConsumed);
                    }
                }
                if dead {
                    if correlated {
                        // Keep the SAME allocated outputs and input reservation.
                        // Only explicit host resume may recreate the extrinsic.
                        // Mixed input presence is ambiguous, not permission to
                        // spend the remaining inputs in a second transaction.
                        if matches!(entry.checkpoint, crate::wal::CheckpointBlock::Pending)
                            || self.inputs_present(entry).await?
                        {
                            self.wal
                                .update_checkpoint(
                                    &entry.entry_id,
                                    crate::wal::CheckpointBlock::Pending,
                                )
                                .await?;
                        }
                        return Ok(Resolution::StillPending);
                    }
                    self.revert_inputs(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::Reverted);
                }
                Ok(Resolution::StillPending)
            }
            WalOperation::IntoExternalAsset => {
                if !matches!(entry.checkpoint, crate::wal::CheckpointBlock::Pending)
                    && self.inputs_consumed(entry).await?
                {
                    self.retire_inputs(entry).await?;
                    self.confirm_surplus_vouchers(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::InputsConsumed);
                }
                if dead {
                    self.revert_inputs(entry).await?;
                    self.delete_surplus_vouchers(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::Reverted);
                }
                Ok(Resolution::StillPending)
            }
            WalOperation::RecycleIntoVoucher => {
                if !matches!(entry.checkpoint, crate::wal::CheckpointBlock::Pending)
                    && self.inputs_consumed(entry).await?
                {
                    self.retire_inputs(entry).await?;
                    self.confirm_surplus_vouchers(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::Landed);
                }
                if dead {
                    self.revert_inputs(entry).await?;
                    self.delete_surplus_vouchers(entry).await?;
                    self.wal.delete(&entry.entry_id).await?;
                    return Ok(Resolution::Reverted);
                }
                Ok(Resolution::StillPending)
            }
            WalOperation::TransferPrepared
            | WalOperation::TransferAccepted
            | WalOperation::TransferCompleted
            | WalOperation::TransferRejected => Ok(Resolution::Receipt),
        }
    }

    async fn inputs_consumed(&self, entry: &TransferWalEntry) -> Result<bool, String> {
        let coin_indices: Vec<u32> = entry
            .payload
            .input_coins
            .iter()
            .map(|c| c.derivation_index)
            .collect();
        let voucher_indices: Vec<u32> = entry
            .payload
            .input_vouchers
            .iter()
            .map(|v| v.derivation_index)
            .collect();
        if coin_indices.is_empty() && voucher_indices.is_empty() {
            return Ok(false);
        }
        let coins = self.probe.coins_present(&coin_indices).await?;
        let vouchers = self.probe.vouchers_present(&voucher_indices).await?;
        if coins.len() != coin_indices.len() || vouchers.len() != voucher_indices.len() {
            return Err("input recovery probe returned an incomplete batch".into());
        }
        Ok(coins.iter().all(|present| !present) && vouchers.iter().all(|present| !present))
    }

    async fn inputs_present(&self, entry: &TransferWalEntry) -> Result<bool, String> {
        let coin_indices = entry
            .payload
            .input_coins
            .iter()
            .map(|coin| coin.derivation_index)
            .collect::<Vec<_>>();
        let voucher_indices = entry
            .payload
            .input_vouchers
            .iter()
            .map(|voucher| voucher.derivation_index)
            .collect::<Vec<_>>();
        let coins = self.probe.coins_present(&coin_indices).await?;
        let vouchers = self.probe.vouchers_present(&voucher_indices).await?;
        if coins.len() != coin_indices.len() || vouchers.len() != voucher_indices.len() {
            return Err("input recovery probe returned an incomplete batch".into());
        }
        Ok(coins.iter().all(|present| *present) && vouchers.iter().all(|present| *present))
    }

    async fn retire_inputs(&self, entry: &TransferWalEntry) -> Result<(), String> {
        for input in &entry.payload.input_coins {
            self.coins
                .set_state(input.derivation_index, CoinState::Spent)
                .await?;
        }
        for input in &entry.payload.input_vouchers {
            self.vouchers
                .set_local_state(input.derivation_index, VoucherLocalState::Spent)
                .await?;
        }
        Ok(())
    }

    /// Split/unload parity: consumed vouchers are removed from the local
    /// purse rather than retained as spent tombstones. Destination outputs
    /// remain spent rows so sync/monitoring can observe their lifecycle.
    async fn retire_transfer_inputs(&self, entry: &TransferWalEntry) -> Result<(), String> {
        for input in &entry.payload.input_coins {
            self.coins
                .set_state(input.derivation_index, CoinState::Spent)
                .await?;
        }
        for input in &entry.payload.input_vouchers {
            self.vouchers.remove(input.derivation_index).await?;
        }
        Ok(())
    }

    async fn revert_inputs(&self, entry: &TransferWalEntry) -> Result<(), String> {
        for input in &entry.payload.input_coins {
            self.coins
                .set_state(input.derivation_index, CoinState::Available)
                .await?;
        }
        for input in &entry.payload.input_vouchers {
            self.vouchers
                .set_local_state(input.derivation_index, VoucherLocalState::Available)
                .await?;
        }
        Ok(())
    }

    async fn confirm_surplus_vouchers(&self, entry: &TransferWalEntry) -> Result<(), String> {
        let indices: Vec<u32> = entry
            .payload
            .output_vouchers
            .iter()
            .map(|v| v.derivation_index)
            .collect();
        if indices.is_empty() {
            return Ok(());
        }
        let present = self.probe.vouchers_present(&indices).await?;
        let known: HashSet<u32> = self
            .vouchers
            .list()
            .await?
            .into_iter()
            .map(|v| v.derivation_index)
            .collect();
        for (reference, present) in entry.payload.output_vouchers.iter().zip(&present) {
            if !*present {
                continue;
            }
            if known.contains(&reference.derivation_index) {
                self.vouchers
                    .set_local_state(reference.derivation_index, VoucherLocalState::Available)
                    .await?;
            } else {
                // Materialize a minimal record; the voucher location
                // service reconciles remote state and readiness
                self.vouchers
                    .upsert(&Voucher {
                        exponent: reference.exponent,
                        derivation_index: reference.derivation_index,
                        allocated_at_ms: 0,
                        ready_at_ms: 0,
                        remote_state: VoucherRemoteState::Unlocated,
                        local_state: VoucherLocalState::Available,
                        privacy: VoucherPrivacyLevel::Degraded,
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn delete_surplus_vouchers(&self, entry: &TransferWalEntry) -> Result<(), String> {
        for output in &entry.payload.output_vouchers {
            self.vouchers.remove(output.derivation_index).await?;
        }
        Ok(())
    }
}

enum Resolution {
    Landed,
    InputsConsumed,
    Reverted,
    StillPending,
    Receipt,
}

/// The coin/voucher indices any journaled entry still references.
fn referenced_indices(entries: &[TransferWalEntry]) -> (HashSet<u32>, HashSet<u32>) {
    let mut coins = HashSet::new();
    let mut vouchers = HashSet::new();
    for entry in entries {
        coins.extend(entry.payload.input_coins.iter().map(|c| c.derivation_index));
        vouchers.extend(
            entry
                .payload
                .input_vouchers
                .iter()
                .map(|v| v.derivation_index),
        );
        vouchers.extend(
            entry
                .payload
                .output_vouchers
                .iter()
                .map(|v| v.derivation_index),
        );
    }
    (coins, vouchers)
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;

    use super::*;
    use crate::repo::{InMemoryCoinRepository, InMemoryVoucherRepository};
    use crate::wal::{CheckpointBlock, WalCoinRef, WalPayload};

    /// In-memory WAL store for the sweep tests.
    #[derive(Default)]
    struct MemWal {
        entries: Mutex<Vec<TransferWalEntry>>,
    }

    #[async_trait]
    impl WalStore for MemWal {
        async fn save(&self, entry: &TransferWalEntry) -> Result<(), String> {
            let mut entries = self.entries.lock();
            entries.retain(|existing| existing.entry_id != entry.entry_id);
            entries.push(entry.clone());
            Ok(())
        }
        async fn save_all(&self, entries: &[TransferWalEntry]) -> Result<(), String> {
            let mut stored = self.entries.lock();
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
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.entry_id == entry_id)
                .ok_or("wal entry not found")?;
            entry.checkpoint = checkpoint;
            Ok(())
        }
        async fn load_all(&self) -> Result<Vec<TransferWalEntry>, String> {
            Ok(self.entries.lock().clone())
        }
        async fn delete(&self, entry_id: &str) -> Result<(), String> {
            self.entries.lock().retain(|e| e.entry_id != entry_id);
            Ok(())
        }
    }

    struct MockProbe {
        finalized: u64,
        canonical: Option<[u8; 32]>,
        coins_on_chain: HashSet<u32>,
        vouchers_on_chain: HashSet<u32>,
        output_exponents: HashMap<u32, i16>,
    }

    impl Default for MockProbe {
        fn default() -> Self {
            Self {
                finalized: 100,
                canonical: Some([1; 32]),
                coins_on_chain: HashSet::new(),
                vouchers_on_chain: HashSet::new(),
                output_exponents: HashMap::new(),
            }
        }
    }

    #[async_trait]
    impl RecoveryChainProbe for MockProbe {
        async fn finalized_block(&self) -> Result<u64, String> {
            Ok(self.finalized)
        }
        async fn canonical_hash(&self, _block_number: u64) -> Result<Option<[u8; 32]>, String> {
            Ok(self.canonical)
        }
        async fn coins_present(&self, indices: &[u32]) -> Result<Vec<bool>, String> {
            Ok(indices
                .iter()
                .map(|i| self.coins_on_chain.contains(i))
                .collect())
        }
        async fn coin_exponents(&self, indices: &[u32]) -> Result<Vec<Option<i16>>, String> {
            Ok(indices
                .iter()
                .map(|index| self.output_exponents.get(index).copied())
                .collect())
        }
        async fn vouchers_present(&self, indices: &[u32]) -> Result<Vec<bool>, String> {
            Ok(indices
                .iter()
                .map(|i| self.vouchers_on_chain.contains(i))
                .collect())
        }
    }

    fn coin(index: u32, state: CoinState) -> Coin {
        Coin {
            exponent: 1,
            derivation_index: index,
            age: Some(1),
            state,
        }
    }

    fn service(
        wal: Arc<MemWal>,
        coins: Arc<InMemoryCoinRepository>,
        vouchers: Arc<InMemoryVoucherRepository>,
        probe: MockProbe,
    ) -> TransferRecoveryService {
        TransferRecoveryService::new(wal, coins, vouchers, Arc::new(probe))
    }

    fn into_coins_entry(checkpoint: CheckpointBlock) -> TransferWalEntry {
        TransferWalEntry {
            entry_id: "e1".into(),
            operation: WalOperation::IntoCoins,
            payload: WalPayload {
                input_coins: vec![WalCoinRef {
                    derivation_index: 1,
                    exponent: 1,
                }],
                input_vouchers: vec![],
                output_coins: vec![WalCoinRef {
                    derivation_index: 50,
                    exponent: 0,
                }],
                output_vouchers: vec![],
                destination_coins: vec![],
            },
            checkpoint,
            created_at_ms: 0,
        }
    }

    async fn coin_state(repo: &InMemoryCoinRepository, index: u32) -> CoinState {
        repo.list()
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.derivation_index == index)
            .unwrap()
            .state
    }

    /// A `Pending` checkpoint means the extrinsic never left the device:
    /// the input coin reverts to `.available` and the entry is dropped.
    #[tokio::test]
    async fn crash_between_wal_write_and_broadcast_reverts_inputs() {
        let wal = Arc::new(MemWal::default());
        wal.save(&into_coins_entry(CheckpointBlock::Pending))
            .await
            .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let vouchers = Arc::new(InMemoryVoucherRepository::default());
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::clone(&vouchers),
            MockProbe::default(),
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.reverted, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::Available);
        assert!(wal.load_all().await.unwrap().is_empty(), "entry retired");
    }

    #[tokio::test]
    async fn into_coins_landed_materializes_outputs_and_retires_inputs() {
        let wal = Arc::new(MemWal::default());
        wal.save(&into_coins_entry(CheckpointBlock::Known {
            number: 90,
            hash: [1; 32],
        }))
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let vouchers = Arc::new(InMemoryVoucherRepository::default());
        let probe = MockProbe {
            coins_on_chain: HashSet::from([50]),
            ..MockProbe::default()
        };
        let service = service(Arc::clone(&wal), Arc::clone(&coins), vouchers, probe);

        let report = service.recover().await.unwrap();
        assert_eq!(report.landed, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::Spent);
        let listed = coins.list().await.unwrap();
        let output = listed.iter().find(|c| c.derivation_index == 50).unwrap();
        assert_eq!(output.state, CoinState::Available);
        assert_eq!(output.age, None, "age unknown until first sync");
        assert!(wal.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unload_recovery_removes_consumed_voucher_and_keeps_destination_spent() {
        let wal = Arc::new(MemWal::default());
        wal.save(&TransferWalEntry {
            entry_id: "unload".into(),
            operation: WalOperation::IntoCoins,
            payload: WalPayload {
                input_coins: vec![],
                input_vouchers: vec![WalCoinRef {
                    derivation_index: 10,
                    exponent: 2,
                }],
                output_coins: vec![WalCoinRef {
                    derivation_index: 50,
                    exponent: 2,
                }],
                output_vouchers: vec![],
                destination_coins: vec![WalCoinRef {
                    derivation_index: 50,
                    exponent: 2,
                }],
            },
            checkpoint: CheckpointBlock::Known {
                number: 90,
                hash: [1; 32],
            },
            created_at_ms: 0,
        })
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::default());
        let vouchers = Arc::new(InMemoryVoucherRepository::with_vouchers([Voucher {
            exponent: 2,
            derivation_index: 10,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: VoucherRemoteState::InRecycler { recycler_index: 1 },
            local_state: VoucherLocalState::PendingTransfer,
            privacy: VoucherPrivacyLevel::Full,
        }]));
        let probe = MockProbe {
            coins_on_chain: HashSet::from([50]),
            ..MockProbe::default()
        };
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::clone(&vouchers),
            probe,
        );
        let report = service.recover().await.unwrap();
        assert_eq!(report.landed, 1);
        assert!(vouchers.list().await.unwrap().is_empty());
        assert_eq!(coin_state(&coins, 50).await, CoinState::Spent);
        assert!(wal.load_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn forked_checkpoint_reverts_immediately() {
        let wal = Arc::new(MemWal::default());
        wal.save(&into_coins_entry(CheckpointBlock::Known {
            number: 90,
            hash: [9; 32], // stored hash ≠ canonical [1; 32]
        }))
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let probe = MockProbe {
            // Input still on-chain (not consumed), output absent,
            // finalized 100 < 90 + 300 (not expired) — ONLY the fork
            // can resolve this entry.
            coins_on_chain: HashSet::from([1]),
            ..MockProbe::default()
        };
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            probe,
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.reverted, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::Available);
    }

    #[tokio::test]
    async fn unresolved_entry_stays_pending() {
        let wal = Arc::new(MemWal::default());
        wal.save(&into_coins_entry(CheckpointBlock::Known {
            number: 90,
            hash: [1; 32],
        }))
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let probe = MockProbe {
            // Input still on-chain, output absent, not expired.
            coins_on_chain: HashSet::from([1]),
            ..MockProbe::default()
        };
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            probe,
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.still_pending, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
        assert_eq!(wal.load_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn expired_entry_reverts_inputs() {
        let wal = Arc::new(MemWal::default());
        wal.save(&into_coins_entry(CheckpointBlock::Known {
            number: 90,
            hash: [1; 32],
        }))
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let probe = MockProbe {
            finalized: 391, // > 90 + 300
            coins_on_chain: HashSet::from([1]),
            ..MockProbe::default()
        };
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            probe,
        );
        let report = service.recover().await.unwrap();
        assert_eq!(report.reverted, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::Available);
    }

    #[tokio::test]
    async fn recycle_into_voucher_confirms_the_surplus_voucher() {
        use crate::model::{VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState};
        let wal = Arc::new(MemWal::default());
        wal.save(&TransferWalEntry {
            entry_id: "r1".into(),
            operation: WalOperation::RecycleIntoVoucher,
            payload: WalPayload {
                input_coins: vec![WalCoinRef {
                    derivation_index: 1,
                    exponent: 1,
                }],
                input_vouchers: vec![],
                output_coins: vec![],
                output_vouchers: vec![WalCoinRef {
                    derivation_index: 70,
                    exponent: 1,
                }],
                destination_coins: vec![],
            },
            checkpoint: CheckpointBlock::Known {
                number: 90,
                hash: [1; 32],
            },
            created_at_ms: 0,
        })
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::Recycling,
        )]));
        let vouchers = Arc::new(InMemoryVoucherRepository::with_vouchers([Voucher {
            exponent: 1,
            derivation_index: 70,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: VoucherRemoteState::Onboarding,
            local_state: VoucherLocalState::PendingOnboarding,
            privacy: VoucherPrivacyLevel::Full,
        }]));
        let probe = MockProbe {
            vouchers_on_chain: HashSet::from([70]),
            // Input coin 1 absent → consumed.
            ..MockProbe::default()
        };
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::clone(&vouchers),
            probe,
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.landed, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::Spent);
        assert_eq!(
            vouchers.list().await.unwrap()[0].local_state,
            VoucherLocalState::Available
        );
    }

    #[tokio::test]
    async fn orphaned_pending_assets_are_restored_or_deleted() {
        use crate::model::{VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState};
        let coins = Arc::new(InMemoryCoinRepository::with_coins([
            coin(1, CoinState::PendingTransfer),
            coin(2, CoinState::Recycling),
            coin(3, CoinState::Spent),
        ]));
        let orphan_voucher = |index, local_state| Voucher {
            exponent: 0,
            derivation_index: index,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: VoucherRemoteState::Unlocated,
            local_state,
            privacy: VoucherPrivacyLevel::Full,
        };
        let vouchers = Arc::new(InMemoryVoucherRepository::with_vouchers([
            orphan_voucher(10, VoucherLocalState::PendingTransfer),
            orphan_voucher(11, VoucherLocalState::PendingOnboarding),
        ]));
        let service = service(
            Arc::new(MemWal::default()),
            Arc::clone(&coins),
            Arc::clone(&vouchers),
            MockProbe::default(),
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.orphans_restored, 3, "two coins + one voucher");
        assert_eq!(report.orphans_deleted, 1, "pending-onboarding voucher");
        assert_eq!(coin_state(&coins, 1).await, CoinState::Available);
        assert_eq!(coin_state(&coins, 2).await, CoinState::Available);
        assert_eq!(
            coin_state(&coins, 3).await,
            CoinState::Spent,
            "spent untouched"
        );
        let listed = vouchers.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].derivation_index, 10);
        assert_eq!(listed[0].local_state, VoucherLocalState::Available);
    }

    /// A journaled still-pending entry PROTECTS its assets from the
    /// orphan sweep.
    #[tokio::test]
    async fn journaled_assets_are_not_swept_as_orphans() {
        let wal = Arc::new(MemWal::default());
        wal.save(&into_coins_entry(CheckpointBlock::Known {
            number: 90,
            hash: [1; 32],
        }))
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let probe = MockProbe {
            coins_on_chain: HashSet::from([1]), // unresolved
            ..MockProbe::default()
        };
        let service = service(
            wal,
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            probe,
        );
        let report = service.recover().await.unwrap();
        assert_eq!(report.orphans_restored, 0);
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
    }

    /// Expanded secrets have no mortality. Even a very old pending
    /// checkpoint remains protected while its coin is present.
    #[tokio::test]
    async fn secret_handoff_never_mortality_reverts_a_present_coin() {
        let wal = Arc::new(MemWal::default());
        wal.save(&TransferWalEntry {
            entry_id: "secret-1".into(),
            operation: WalOperation::SecretHandoff,
            payload: WalPayload {
                input_coins: vec![WalCoinRef {
                    derivation_index: 1,
                    exponent: 1,
                }],
                ..WalPayload::default()
            },
            checkpoint: CheckpointBlock::Pending,
            created_at_ms: 0,
        })
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            MockProbe {
                finalized: u64::MAX,
                coins_on_chain: HashSet::from([1]),
                ..MockProbe::default()
            },
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.still_pending, 1);
        assert_eq!(report.reverted, 0);
        assert_eq!(report.orphans_restored, 0);
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
        assert_eq!(wal.load_all().await.unwrap().len(), 1);
    }

    /// Chain absence is the sole terminal proof for an out-of-band
    /// secret handoff.
    #[tokio::test]
    async fn secret_handoff_absence_retires_inputs_and_journal() {
        let wal = Arc::new(MemWal::default());
        wal.save(&TransferWalEntry {
            entry_id: "secret-2".into(),
            operation: WalOperation::SecretHandoff,
            payload: WalPayload {
                input_coins: vec![WalCoinRef {
                    derivation_index: 1,
                    exponent: 1,
                }],
                ..WalPayload::default()
            },
            checkpoint: CheckpointBlock::Pending,
            created_at_ms: 0,
        })
        .await
        .unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let service = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            MockProbe::default(),
        );

        let report = service.recover().await.unwrap();
        assert_eq!(report.inputs_consumed, 1);
        assert_eq!(coin_state(&coins, 1).await, CoinState::Spent);
        assert!(wal.load_all().await.unwrap().is_empty());
    }
    async fn operation_fixture(
        state: WalOperation,
        checkpoint: CheckpointBlock,
        probe: MockProbe,
    ) -> (
        TransferRecoveryService,
        Arc<MemWal>,
        Arc<InMemoryCoinRepository>,
    ) {
        let mut child = into_coins_entry(checkpoint);
        child.entry_id = crate::wal::operation_entry_id("host-payment", "split");
        child.operation = WalOperation::Split;
        child.payload.output_coins.push(WalCoinRef {
            derivation_index: 51,
            exponent: 0,
        });
        child.payload.destination_coins = child.payload.output_coins.clone();
        let parent = TransferWalEntry {
            entry_id: crate::wal::operation_entry_id("host-payment", "parent"),
            operation: state,
            payload: WalPayload {
                output_coins: child.payload.destination_coins.clone(),
                ..WalPayload::default()
            },
            checkpoint: CheckpointBlock::Pending,
            created_at_ms: 0,
        };
        let wal = Arc::new(MemWal::default());
        wal.save_all(&[parent, child]).await.unwrap();
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(
            1,
            CoinState::PendingTransfer,
        )]));
        let recovery = service(
            Arc::clone(&wal),
            Arc::clone(&coins),
            Arc::new(InMemoryVoucherRepository::default()),
            probe,
        );
        (recovery, wal, coins)
    }

    #[tokio::test]
    async fn prepared_transport_ambiguity_never_releases_a_pending_chain_input() {
        let (service, wal, coins) = operation_fixture(
            WalOperation::TransferPrepared,
            CheckpointBlock::Pending,
            MockProbe {
                finalized: 10_000,
                ..MockProbe::default()
            },
        )
        .await;
        let report = service.recover().await.unwrap();
        assert_eq!(report.reverted, 0);
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
        let rows = wal.load_operation("host-payment").await.unwrap();
        assert!(
            rows.iter()
                .any(|entry| entry.operation == WalOperation::TransferPrepared)
        );
        assert!(
            rows.iter()
                .any(|entry| entry.operation == WalOperation::Split)
        );
    }

    #[tokio::test]
    async fn expired_accepted_plan_is_resumeable_without_unlocking_or_reallocating() {
        let (service, wal, coins) = operation_fixture(
            WalOperation::TransferAccepted,
            CheckpointBlock::Known {
                number: 1,
                hash: [1; 32],
            },
            MockProbe {
                finalized: 1_000,
                coins_on_chain: HashSet::from([1]),
                ..MockProbe::default()
            },
        )
        .await;
        let original = wal
            .load_operation("host-payment")
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.operation == WalOperation::Split)
            .unwrap();
        service.recover().await.unwrap();
        service.recover().await.unwrap();
        let restored = wal
            .load_operation("host-payment")
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.operation == WalOperation::Split)
            .unwrap();
        assert_eq!(restored.entry_id, original.entry_id);
        assert_eq!(restored.payload, original.payload);
        assert_eq!(restored.checkpoint, CheckpointBlock::Pending);
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
    }

    #[tokio::test]
    async fn consumed_inputs_without_output_evidence_cannot_complete_an_operation() {
        let checkpoint = CheckpointBlock::Known {
            number: 1,
            hash: [1; 32],
        };
        let (service, wal, coins) = operation_fixture(
            WalOperation::TransferAccepted,
            checkpoint,
            MockProbe {
                finalized: 1_000,
                ..MockProbe::default()
            },
        )
        .await;
        service.recover().await.unwrap();
        let rows = wal.load_operation("host-payment").await.unwrap();
        assert!(
            rows.iter()
                .any(|entry| entry.operation == WalOperation::TransferAccepted)
        );
        let child = rows
            .iter()
            .find(|entry| entry.operation == WalOperation::Split)
            .unwrap();
        assert_eq!(child.checkpoint, checkpoint);
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
    }

    #[tokio::test]
    async fn landed_operation_keeps_its_receipt_after_repeated_recovery() {
        let (service, wal, coins) = operation_fixture(
            WalOperation::TransferAccepted,
            CheckpointBlock::Known {
                number: 1,
                hash: [1; 32],
            },
            MockProbe {
                coins_on_chain: HashSet::from([50, 51]),
                output_exponents: HashMap::from([(50, 0), (51, 0)]),
                ..MockProbe::default()
            },
        )
        .await;
        service.recover().await.unwrap();
        service.recover().await.unwrap();
        assert_eq!(coin_state(&coins, 1).await, CoinState::Spent);
        assert_eq!(coin_state(&coins, 50).await, CoinState::Spent);
        let rows = wal.load_operation("host-payment").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].operation, WalOperation::TransferCompleted);
        assert_eq!(rows[0].payload.output_coins[0].derivation_index, 50);
    }

    #[tokio::test]
    async fn partial_or_wrong_output_evidence_cannot_complete_or_rebroadcast() {
        for outputs in [HashMap::from([(50, 0)]), HashMap::from([(50, 0), (51, 4)])] {
            let checkpoint = CheckpointBlock::Known {
                number: 1,
                hash: [1; 32],
            };
            let (service, wal, coins) = operation_fixture(
                WalOperation::TransferAccepted,
                checkpoint,
                MockProbe {
                    finalized: 1_000,
                    coins_on_chain: HashSet::from([1, 50, 51]),
                    output_exponents: outputs,
                    ..MockProbe::default()
                },
            )
            .await;
            service.recover().await.unwrap();
            let rows = wal.load_operation("host-payment").await.unwrap();
            assert!(
                rows.iter()
                    .any(|entry| entry.operation == WalOperation::TransferAccepted)
            );
            assert_eq!(
                rows.iter()
                    .find(|entry| entry.operation == WalOperation::Split)
                    .unwrap()
                    .checkpoint,
                checkpoint
            );
            assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
        }
    }
}
