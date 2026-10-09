// SPDX-License-Identifier: AGPL-3.0-only
//! Host snapshot implementations of Brevity Coinage's repository contracts.

use std::collections::BTreeSet;

use async_trait::async_trait;
use truapi_coinage::{
    claim_plan::{ClaimPlan, ClaimPlanStatus, ClaimPlanStore},
    index_store::{CoinageIndexStore, IndexKind},
    model::{Coin, CoinState, Voucher, VoucherLocalState, VoucherRemoteState},
    repo::{CoinRepository, TransferStateCommitter, VoucherRepository},
    wal::{CheckpointBlock, TransferWalEntry, WalOperation, WalStore},
};

use super::{HostCoinageStore, MAX_ITEMS, Snapshot, StoreError, codec};

impl Snapshot {
    fn put_coin(&mut self, coin: Coin) -> Result<(), StoreError> {
        if self
            .coins
            .get(&coin.derivation_index)
            .is_some_and(|old| old.exponent != coin.exponent)
        {
            return Err(StoreError::Conflict);
        }
        codec::advance(&mut self.coin_index, coin.derivation_index);
        self.coins.insert(coin.derivation_index, coin);
        Ok(())
    }

    fn put_voucher(&mut self, voucher: Voucher) -> Result<(), StoreError> {
        if self
            .vouchers
            .get(&voucher.derivation_index)
            .is_some_and(|old| old.exponent != voucher.exponent)
        {
            return Err(StoreError::Conflict);
        }
        codec::advance(&mut self.voucher_index, voucher.derivation_index);
        self.vouchers.insert(voucher.derivation_index, voucher);
        Ok(())
    }

    fn put_wal(&mut self, entry: TransferWalEntry) -> Result<(), StoreError> {
        codec::validate_id(&entry.entry_id)?;
        if let Some(old) = self.wal.get(&entry.entry_id) {
            // Receipt/checkpoint progress is mutable, spend identity is not.
            let transition = old.operation == entry.operation
                || matches!(
                    (old.operation, entry.operation),
                    (
                        WalOperation::TransferPrepared,
                        WalOperation::TransferAccepted | WalOperation::TransferRejected
                    ) | (
                        WalOperation::TransferAccepted,
                        WalOperation::TransferCompleted
                    )
                );
            if old.payload != entry.payload
                || old.created_at_ms != entry.created_at_ms
                || !transition
            {
                return Err(StoreError::Conflict);
            }
        }
        for coin in entry
            .payload
            .input_coins
            .iter()
            .chain(&entry.payload.output_coins)
            .chain(&entry.payload.destination_coins)
        {
            codec::advance(&mut self.coin_index, coin.derivation_index);
        }
        for voucher in entry
            .payload
            .input_vouchers
            .iter()
            .chain(&entry.payload.output_vouchers)
        {
            codec::advance(&mut self.voucher_index, voucher.derivation_index);
        }
        self.wal.insert(entry.entry_id.clone(), entry);
        Ok(())
    }

    fn counter(&mut self, kind: IndexKind) -> &mut Option<u32> {
        match kind {
            IndexKind::Coin => &mut self.coin_index,
            IndexKind::Voucher => &mut self.voucher_index,
        }
    }
}

fn unique(indices: &[u32]) -> Result<(), StoreError> {
    if indices.len() > MAX_ITEMS {
        return Err(StoreError::Capacity);
    }
    let mut seen = BTreeSet::new();
    if indices.iter().all(|index| seen.insert(*index)) {
        Ok(())
    } else {
        Err(StoreError::Conflict)
    }
}

#[async_trait]
impl CoinRepository for HostCoinageStore {
    async fn list(&self) -> Result<Vec<Coin>, String> {
        Ok(self
            .checked_state()
            .await
            .map_err(|e| e.to_string())?
            .coins
            .values()
            .cloned()
            .collect())
    }

    async fn upsert(&self, coin: &Coin) -> Result<(), String> {
        let coin = coin.clone();
        self.mutate(move |state| state.put_coin(coin))
            .await
            .map_err(|error| error.to_string())
    }

    async fn set_state(&self, derivation_index: u32, new_state: CoinState) -> Result<(), String> {
        self.mutate(move |state| {
            state
                .coins
                .get_mut(&derivation_index)
                .ok_or(StoreError::Missing)?
                .state = new_state;
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn remove(&self, derivation_index: u32) -> Result<(), String> {
        self.mutate(move |state| {
            state.coins.remove(&derivation_index);
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl VoucherRepository for HostCoinageStore {
    async fn list(&self) -> Result<Vec<Voucher>, String> {
        Ok(self
            .checked_state()
            .await
            .map_err(|e| e.to_string())?
            .vouchers
            .values()
            .cloned()
            .collect())
    }

    async fn upsert(&self, voucher: &Voucher) -> Result<(), String> {
        let voucher = voucher.clone();
        self.mutate(move |state| state.put_voucher(voucher))
            .await
            .map_err(|error| error.to_string())
    }

    async fn set_local_state(
        &self,
        derivation_index: u32,
        new_state: VoucherLocalState,
    ) -> Result<(), String> {
        self.mutate(move |state| {
            state
                .vouchers
                .get_mut(&derivation_index)
                .ok_or(StoreError::Missing)?
                .local_state = new_state;
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn set_remote_state(
        &self,
        derivation_index: u32,
        new_state: VoucherRemoteState,
    ) -> Result<(), String> {
        self.mutate(move |state| {
            state
                .vouchers
                .get_mut(&derivation_index)
                .ok_or(StoreError::Missing)?
                .remote_state = new_state;
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn remove(&self, derivation_index: u32) -> Result<(), String> {
        self.mutate(move |state| {
            state.vouchers.remove(&derivation_index);
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl WalStore for HostCoinageStore {
    async fn save(&self, entry: &TransferWalEntry) -> Result<(), String> {
        let entry = entry.clone();
        self.mutate(move |state| state.put_wal(entry))
            .await
            .map_err(|error| error.to_string())
    }

    async fn save_all(&self, entries: &[TransferWalEntry]) -> Result<(), String> {
        if entries.len() > super::MAX_RECORDS {
            return Err(StoreError::Capacity.to_string());
        }
        let entries = entries.to_vec();
        self.mutate(move |state| {
            let mut seen = BTreeSet::new();
            for entry in entries {
                if !seen.insert(entry.entry_id.clone()) {
                    return Err(StoreError::Conflict);
                }
                state.put_wal(entry)?;
            }
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn update_checkpoint(
        &self,
        entry_id: &str,
        checkpoint: CheckpointBlock,
    ) -> Result<(), String> {
        let entry_id = entry_id.to_owned();
        self.mutate(move |state| {
            let entry = state.wal.get_mut(&entry_id).ok_or(StoreError::Missing)?;
            if entry.checkpoint != CheckpointBlock::Pending && entry.checkpoint != checkpoint {
                return Err(StoreError::Conflict);
            }
            entry.checkpoint = checkpoint;
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn load_all(&self) -> Result<Vec<TransferWalEntry>, String> {
        Ok(self
            .checked_state()
            .await
            .map_err(|e| e.to_string())?
            .wal
            .values()
            .cloned()
            .collect())
    }

    async fn delete(&self, entry_id: &str) -> Result<(), String> {
        let entry_id = entry_id.to_owned();
        self.mutate(move |state| {
            state.wal.remove(&entry_id);
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl ClaimPlanStore for HostCoinageStore {
    async fn save(&self, plan: &ClaimPlan) -> Result<(), String> {
        let plan = plan.clone();
        self.mutate(move |state| {
            for entry in &plan.entries {
                codec::advance(&mut state.coin_index, entry.derivation_index);
            }
            state.plans.insert(plan.memo_key, plan);
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn plan(&self, memo_key: &[u8; 32]) -> Result<Option<ClaimPlan>, String> {
        Ok(self
            .checked_state()
            .await
            .map_err(|e| e.to_string())?
            .plans
            .get(memo_key)
            .cloned())
    }

    async fn load_all(&self) -> Result<Vec<ClaimPlan>, String> {
        Ok(self
            .checked_state()
            .await
            .map_err(|e| e.to_string())?
            .plans
            .values()
            .cloned()
            .collect())
    }

    async fn update_status(
        &self,
        memo_key: &[u8; 32],
        status: ClaimPlanStatus,
        claimed_amount: Option<u128>,
    ) -> Result<(), String> {
        let memo_key = *memo_key;
        self.mutate(move |state| {
            let plan = state.plans.get_mut(&memo_key).ok_or(StoreError::Missing)?;
            plan.status = status;
            plan.claimed_amount = claimed_amount;
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn remove(&self, memo_key: &[u8; 32]) -> Result<(), String> {
        let memo_key = *memo_key;
        self.mutate(move |state| {
            state.plans.remove(&memo_key);
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl CoinageIndexStore for HostCoinageStore {
    async fn get_next_index(&self, kind: IndexKind) -> Result<u32, String> {
        self.mutate(move |state| {
            let counter = state.counter(kind);
            let next = match *counter {
                None => 0,
                Some(current) => current.checked_add(1).ok_or(StoreError::Exhausted)?,
            };
            *counter = Some(next);
            Ok(next)
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn current_index(&self, kind: IndexKind) -> Result<Option<u32>, String> {
        let state = self.checked_state().await.map_err(|e| e.to_string())?;
        Ok(match kind {
            IndexKind::Coin => state.coin_index,
            IndexKind::Voucher => state.voucher_index,
        })
    }

    async fn set_index(&self, kind: IndexKind, index: u32) -> Result<(), String> {
        self.mutate(move |state| {
            let counter = state.counter(kind);
            if counter.is_some_and(|old| index < old) {
                return Err(StoreError::Conflict);
            }
            *counter = Some(index);
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl TransferStateCommitter for HostCoinageStore {
    async fn reserve(&self, coins: &[u32], vouchers: &[u32]) -> Result<(), String> {
        unique(coins)
            .and_then(|()| unique(vouchers))
            .map_err(|error| error.to_string())?;
        let coins = coins.to_vec();
        let vouchers = vouchers.to_vec();
        self.mutate(move |state| {
            for index in coins {
                let coin = state.coins.get_mut(&index).ok_or(StoreError::Missing)?;
                if coin.state != CoinState::Available {
                    return Err(StoreError::Conflict);
                }
                coin.state = CoinState::PendingTransfer;
            }
            for index in vouchers {
                let voucher = state.vouchers.get_mut(&index).ok_or(StoreError::Missing)?;
                if voucher.local_state != VoucherLocalState::Available {
                    return Err(StoreError::Conflict);
                }
                voucher.local_state = VoucherLocalState::PendingTransfer;
            }
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn revert(&self, coins: &[u32], vouchers: &[u32]) -> Result<(), String> {
        unique(coins)
            .and_then(|()| unique(vouchers))
            .map_err(|error| error.to_string())?;
        let coins = coins.to_vec();
        let vouchers = vouchers.to_vec();
        self.mutate(move |state| {
            for index in coins {
                if let Some(coin) = state.coins.get_mut(&index)
                    && coin.state == CoinState::PendingTransfer
                {
                    coin.state = CoinState::Available;
                }
            }
            for index in vouchers {
                if let Some(voucher) = state.vouchers.get_mut(&index)
                    && voucher.local_state == VoucherLocalState::PendingTransfer
                {
                    voucher.local_state = VoucherLocalState::Available;
                }
            }
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }

    async fn commit(
        &self,
        spent_coins: &[u32],
        spent_vouchers: &[u32],
        change: &[Coin],
        destination: &[Coin],
    ) -> Result<(), String> {
        unique(spent_coins)
            .and_then(|()| unique(spent_vouchers))
            .map_err(|error| error.to_string())?;
        if change.len().saturating_add(destination.len()) > MAX_ITEMS {
            return Err(StoreError::Capacity.to_string());
        }
        let spent_coins = spent_coins.to_vec();
        let spent_vouchers = spent_vouchers.to_vec();
        let change = change.to_vec();
        let destination = destination.to_vec();
        self.mutate(move |state| {
            let mut indices: BTreeSet<_> = spent_coins.iter().copied().collect();
            for coin in change.iter().chain(&destination) {
                if !indices.insert(coin.derivation_index) {
                    return Err(StoreError::Conflict);
                }
                if state.coins.contains_key(&coin.derivation_index) {
                    return Err(StoreError::Conflict);
                }
            }
            for index in spent_coins {
                let coin = state.coins.get_mut(&index).ok_or(StoreError::Missing)?;
                if coin.state != CoinState::PendingTransfer {
                    return Err(StoreError::Conflict);
                }
                coin.state = CoinState::Spent;
            }
            for index in spent_vouchers {
                let voucher = state.vouchers.get(&index).ok_or(StoreError::Missing)?;
                if voucher.local_state != VoucherLocalState::PendingTransfer {
                    return Err(StoreError::Conflict);
                }
                state.vouchers.remove(&index);
            }
            for mut coin in change {
                coin.state = CoinState::Available;
                state.put_coin(coin)?;
            }
            for mut coin in destination {
                coin.state = CoinState::Spent;
                state.put_coin(coin)?;
            }
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
    }
}
