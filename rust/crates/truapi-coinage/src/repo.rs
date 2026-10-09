// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! Coin/voucher repository contracts and the transfer reservation
//!
//! The repositories are the local-DB effect boundary: durable implementations
//! belong to the Host; in-memory implementations here serve tests and the
//! executable contract. [`TransferContext`] is the reserve → process /
//! revert lifecycle wrapped around one transfer: reserve marks inputs
//! `PendingTransfer` before any extrinsic, process retires spent inputs,
//! revert restores whatever is still pending after a failure.

use parking_lot::Mutex;
use std::collections::HashMap;

use async_trait::async_trait;

use crate::model::{Coin, CoinState, Voucher, VoucherLocalState, VoucherRemoteState};

/// Local coin persistence effect.
#[async_trait]
pub trait CoinRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Coin>, String>;

    /// Insert or replace by `derivation_index`.
    async fn upsert(&self, coin: &Coin) -> Result<(), String>;

    /// Moves one coin's state; unknown index is an error (a recovery
    /// sweep must never silently miss).
    async fn set_state(&self, derivation_index: u32, state: CoinState) -> Result<(), String>;

    async fn remove(&self, derivation_index: u32) -> Result<(), String>;
}

/// Local voucher persistence effect.
#[async_trait]
pub trait VoucherRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Voucher>, String>;

    async fn upsert(&self, voucher: &Voucher) -> Result<(), String>;

    async fn set_local_state(
        &self,
        derivation_index: u32,
        state: VoucherLocalState,
    ) -> Result<(), String>;

    /// Updates only the chain-location projection. Unknown indices are
    /// errors so a subscription cannot silently lose a mapper request.
    async fn set_remote_state(
        &self,
        derivation_index: u32,
        state: VoucherRemoteState,
    ) -> Result<(), String>;

    async fn remove(&self, derivation_index: u32) -> Result<(), String>;
}

/// Optional persistence fast path for one logical strategy/group commit.
/// SQLite uses this to make output insertion, input retirement, and voucher
/// deletion one transaction; in-memory/domain-only compositions can retain
/// the repository fallback.
#[async_trait]
pub trait TransferStateCommitter: Send + Sync {
    async fn reserve(&self, coins: &[u32], vouchers: &[u32]) -> Result<(), String>;

    async fn revert(&self, coins: &[u32], vouchers: &[u32]) -> Result<(), String>;

    async fn commit(
        &self,
        spent_coins: &[u32],
        spent_vouchers: &[u32],
        change: &[Coin],
        destination: &[Coin],
    ) -> Result<(), String>;
}

/// Mutex-guarded in-memory [`CoinRepository`].
#[derive(Default)]
pub struct InMemoryCoinRepository {
    coins: Mutex<HashMap<u32, Coin>>,
}

impl InMemoryCoinRepository {
    pub fn with_coins(coins: impl IntoIterator<Item = Coin>) -> Self {
        Self {
            coins: Mutex::new(coins.into_iter().map(|c| (c.derivation_index, c)).collect()),
        }
    }
}

#[async_trait]
impl CoinRepository for InMemoryCoinRepository {
    async fn list(&self) -> Result<Vec<Coin>, String> {
        let mut coins: Vec<Coin> = self.coins.lock().values().cloned().collect();
        coins.sort_by_key(|c| c.derivation_index);
        Ok(coins)
    }

    async fn upsert(&self, coin: &Coin) -> Result<(), String> {
        self.coins
            .lock()
            .insert(coin.derivation_index, coin.clone());
        Ok(())
    }

    async fn set_state(&self, derivation_index: u32, state: CoinState) -> Result<(), String> {
        match self.coins.lock().get_mut(&derivation_index) {
            Some(coin) => {
                coin.state = state;
                Ok(())
            }
            None => Err(format!("coin {derivation_index} not found")),
        }
    }

    async fn remove(&self, derivation_index: u32) -> Result<(), String> {
        self.coins.lock().remove(&derivation_index);
        Ok(())
    }
}

/// Mutex-guarded in-memory [`VoucherRepository`].
#[derive(Default)]
pub struct InMemoryVoucherRepository {
    vouchers: Mutex<HashMap<u32, Voucher>>,
}

impl InMemoryVoucherRepository {
    pub fn with_vouchers(vouchers: impl IntoIterator<Item = Voucher>) -> Self {
        Self {
            vouchers: Mutex::new(
                vouchers
                    .into_iter()
                    .map(|v| (v.derivation_index, v))
                    .collect(),
            ),
        }
    }
}

#[async_trait]
impl VoucherRepository for InMemoryVoucherRepository {
    async fn list(&self) -> Result<Vec<Voucher>, String> {
        let mut vouchers: Vec<Voucher> = self.vouchers.lock().values().cloned().collect();
        vouchers.sort_by_key(|v| v.derivation_index);
        Ok(vouchers)
    }

    async fn upsert(&self, voucher: &Voucher) -> Result<(), String> {
        self.vouchers
            .lock()
            .insert(voucher.derivation_index, voucher.clone());
        Ok(())
    }

    async fn set_local_state(
        &self,
        derivation_index: u32,
        state: VoucherLocalState,
    ) -> Result<(), String> {
        match self.vouchers.lock().get_mut(&derivation_index) {
            Some(voucher) => {
                voucher.local_state = state;
                Ok(())
            }
            None => Err(format!("voucher {derivation_index} not found")),
        }
    }

    async fn set_remote_state(
        &self,
        derivation_index: u32,
        state: VoucherRemoteState,
    ) -> Result<(), String> {
        match self.vouchers.lock().get_mut(&derivation_index) {
            Some(voucher) => {
                voucher.remote_state = state;
                Ok(())
            }
            None => Err(format!("voucher {derivation_index} not found")),
        }
    }

    async fn remove(&self, derivation_index: u32) -> Result<(), String> {
        self.vouchers.lock().remove(&derivation_index);
        Ok(())
    }
}

use std::sync::Arc;

/// The reserve → process / revert lifecycle around one transfer
///
/// - [`reserve`](Self::reserve) marks inputs `PendingTransfer` BEFORE the
///   strategy runs — a process death here is caught by orphan recovery;
/// - [`process`](Self::process) retires spent inputs, removing them from
///   the pending sets before the first await; if a later `set_state` call
///   fails, the removed ids are re-appended so `revert` can still find
///   them;
/// - [`revert`](Self::revert) restores everything still pending back to
///   `Available`.
pub struct TransferContext {
    coins: Arc<dyn CoinRepository>,
    vouchers: Arc<dyn VoucherRepository>,
    pending_coins: Mutex<Vec<u32>>,
    pending_vouchers: Mutex<Vec<u32>>,
    committer: Option<Arc<dyn TransferStateCommitter>>,
}

impl TransferContext {
    pub fn new(coins: Arc<dyn CoinRepository>, vouchers: Arc<dyn VoucherRepository>) -> Self {
        Self {
            coins,
            vouchers,
            pending_coins: Mutex::new(Vec::new()),
            pending_vouchers: Mutex::new(Vec::new()),
            committer: None,
        }
    }

    pub fn with_committer(mut self, committer: Arc<dyn TransferStateCommitter>) -> Self {
        self.committer = Some(committer);
        self
    }

    pub async fn reserve(&self, coins: &[u32], vouchers: &[u32]) -> Result<(), String> {
        if let Some(committer) = &self.committer {
            committer.reserve(coins, vouchers).await?;
            self.pending_coins.lock().extend_from_slice(coins);
            self.pending_vouchers.lock().extend_from_slice(vouchers);
            return Ok(());
        }
        let mut reserved_coins = Vec::new();
        for &index in coins {
            if let Err(error) = self
                .coins
                .set_state(index, CoinState::PendingTransfer)
                .await
            {
                for reserved in reserved_coins {
                    let _ = self.coins.set_state(reserved, CoinState::Available).await;
                }
                self.pending_coins.lock().clear();
                return Err(error);
            }
            reserved_coins.push(index);
            self.pending_coins.lock().push(index);
        }
        let mut reserved_vouchers = Vec::new();
        for &index in vouchers {
            if let Err(error) = self
                .vouchers
                .set_local_state(index, VoucherLocalState::PendingTransfer)
                .await
            {
                for reserved in reserved_vouchers {
                    let _ = self
                        .vouchers
                        .set_local_state(reserved, VoucherLocalState::Available)
                        .await;
                }
                for reserved in reserved_coins {
                    let _ = self.coins.set_state(reserved, CoinState::Available).await;
                }
                self.pending_coins.lock().clear();
                self.pending_vouchers.lock().clear();
                return Err(error);
            }
            reserved_vouchers.push(index);
            self.pending_vouchers.lock().push(index);
        }
        Ok(())
    }

    pub async fn process(&self, spent_coins: &[u32], spent_vouchers: &[u32]) -> Result<(), String> {
        let removed_coins: Vec<u32> = {
            let mut pending = self.pending_coins.lock();
            let removed = pending
                .iter()
                .copied()
                .filter(|index| spent_coins.contains(index))
                .collect();
            pending.retain(|index| !spent_coins.contains(index));
            removed
        };
        let removed_vouchers: Vec<u32> = {
            let mut pending = self.pending_vouchers.lock();
            let removed = pending
                .iter()
                .copied()
                .filter(|index| spent_vouchers.contains(index))
                .collect();
            pending.retain(|index| !spent_vouchers.contains(index));
            removed
        };

        let restore = |this: &Self| {
            this.pending_coins.lock().extend_from_slice(&removed_coins);
            this.pending_vouchers
                .lock()
                .extend_from_slice(&removed_vouchers);
        };
        for &index in spent_coins {
            if let Err(error) = self.coins.set_state(index, CoinState::Spent).await {
                restore(self);
                return Err(error);
            }
        }
        for &index in spent_vouchers {
            if let Err(error) = self
                .vouchers
                .set_local_state(index, VoucherLocalState::Spent)
                .await
            {
                restore(self);
                return Err(error);
            }
        }
        Ok(())
    }

    pub async fn process_outputs(
        &self,
        spent_coins: &[u32],
        spent_vouchers: &[u32],
        change: &[Coin],
        destination: &[Coin],
    ) -> Result<(), String> {
        let removed_coins: Vec<u32> = {
            let mut pending = self.pending_coins.lock();
            let removed = pending
                .iter()
                .copied()
                .filter(|index| spent_coins.contains(index))
                .collect();
            pending.retain(|index| !spent_coins.contains(index));
            removed
        };
        let removed_vouchers: Vec<u32> = {
            let mut pending = self.pending_vouchers.lock();
            let removed = pending
                .iter()
                .copied()
                .filter(|index| spent_vouchers.contains(index))
                .collect();
            pending.retain(|index| !spent_vouchers.contains(index));
            removed
        };
        let restore = |this: &Self| {
            this.pending_coins.lock().extend_from_slice(&removed_coins);
            this.pending_vouchers
                .lock()
                .extend_from_slice(&removed_vouchers);
        };

        let commit = async {
            if let Some(committer) = &self.committer {
                return committer
                    .commit(spent_coins, spent_vouchers, change, destination)
                    .await;
            }
            for coin in change {
                let mut coin = coin.clone();
                coin.state = CoinState::Available;
                self.coins.upsert(&coin).await?;
            }
            for coin in destination {
                let mut coin = coin.clone();
                coin.state = CoinState::Spent;
                self.coins.upsert(&coin).await?;
            }
            for &index in spent_coins {
                self.coins.set_state(index, CoinState::Spent).await?;
            }
            for &index in spent_vouchers {
                self.vouchers.remove(index).await?;
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = commit {
            restore(self);
            return Err(error);
        }
        Ok(())
    }

    /// Restores only the supplied still-pending inputs. Used after a
    /// definitive pre-broadcast failure so another recycler group with a
    /// known checkpoint remains reserved for recovery.
    pub async fn revert_inputs(&self, coins: &[u32], vouchers: &[u32]) -> Result<(), String> {
        let coins = {
            let mut pending = self.pending_coins.lock();
            let selected = pending
                .iter()
                .copied()
                .filter(|index| coins.contains(index))
                .collect::<Vec<_>>();
            pending.retain(|index| !coins.contains(index));
            selected
        };
        let vouchers = {
            let mut pending = self.pending_vouchers.lock();
            let selected = pending
                .iter()
                .copied()
                .filter(|index| vouchers.contains(index))
                .collect::<Vec<_>>();
            pending.retain(|index| !vouchers.contains(index));
            selected
        };
        if let Some(committer) = &self.committer {
            if let Err(error) = committer.revert(&coins, &vouchers).await {
                self.pending_coins.lock().extend_from_slice(&coins);
                self.pending_vouchers.lock().extend_from_slice(&vouchers);
                return Err(error);
            }
            return Ok(());
        }
        for index in coins {
            self.coins.set_state(index, CoinState::Available).await?;
        }
        for index in vouchers {
            self.vouchers
                .set_local_state(index, VoucherLocalState::Available)
                .await?;
        }
        Ok(())
    }

    pub async fn revert(&self) -> Result<(), String> {
        let coins: Vec<u32> = std::mem::take(&mut *self.pending_coins.lock());
        let vouchers: Vec<u32> = std::mem::take(&mut *self.pending_vouchers.lock());
        if let Some(committer) = &self.committer {
            if let Err(error) = committer.revert(&coins, &vouchers).await {
                self.pending_coins.lock().extend_from_slice(&coins);
                self.pending_vouchers.lock().extend_from_slice(&vouchers);
                return Err(error);
            }
            return Ok(());
        }
        for index in coins {
            self.coins.set_state(index, CoinState::Available).await?;
        }
        for index in vouchers {
            self.vouchers
                .set_local_state(index, VoucherLocalState::Available)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{VoucherPrivacyLevel, VoucherRemoteState};

    fn coin(index: u32) -> Coin {
        Coin {
            exponent: 0,
            derivation_index: index,
            age: Some(1),
            state: CoinState::Available,
        }
    }

    fn voucher(index: u32) -> Voucher {
        Voucher {
            exponent: 0,
            derivation_index: index,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: VoucherRemoteState::InRecycler { recycler_index: 0 },
            local_state: VoucherLocalState::Available,
            privacy: VoucherPrivacyLevel::Full,
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

    #[tokio::test]
    async fn reserve_process_revert_lifecycle() {
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(1), coin(2)]));
        let vouchers = Arc::new(InMemoryVoucherRepository::with_vouchers([voucher(10)]));
        let context = TransferContext::new(
            Arc::clone(&coins) as Arc<_>,
            Arc::clone(&vouchers) as Arc<_>,
        );

        context.reserve(&[1, 2], &[10]).await.unwrap();
        assert_eq!(coin_state(&coins, 1).await, CoinState::PendingTransfer);
        assert_eq!(coin_state(&coins, 2).await, CoinState::PendingTransfer);

        // One coin is processed (spent); the rest reverts.
        context.process(&[1], &[]).await.unwrap();
        context.revert().await.unwrap();

        assert_eq!(
            coin_state(&coins, 1).await,
            CoinState::Spent,
            "processed stays spent"
        );
        assert_eq!(
            coin_state(&coins, 2).await,
            CoinState::Available,
            "unprocessed reverts"
        );
        assert_eq!(
            vouchers.list().await.unwrap()[0].local_state,
            VoucherLocalState::Available,
            "unprocessed voucher reverts"
        );
    }

    #[tokio::test]
    async fn failed_process_keeps_items_revertable() {
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(1)]));
        let vouchers = Arc::new(InMemoryVoucherRepository::default());
        let context = TransferContext::new(Arc::clone(&coins) as Arc<_>, vouchers);

        context.reserve(&[1], &[]).await.unwrap();
        // Index 99 does not exist — set_state fails mid-process.
        assert!(context.process(&[99], &[]).await.is_err());
        context.revert().await.unwrap();
        assert_eq!(coin_state(&coins, 1).await, CoinState::Available);
    }

    #[tokio::test]
    async fn double_revert_is_a_no_op() {
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(1)]));
        let vouchers = Arc::new(InMemoryVoucherRepository::default());
        let context = TransferContext::new(Arc::clone(&coins) as Arc<_>, vouchers);
        context.reserve(&[1], &[]).await.unwrap();
        context.revert().await.unwrap();
        context.revert().await.unwrap();
        assert_eq!(coin_state(&coins, 1).await, CoinState::Available);
    }

    #[tokio::test]
    async fn process_outputs_persists_change_spends_destination_and_deletes_vouchers() {
        let coins = Arc::new(InMemoryCoinRepository::with_coins([coin(1)]));
        let vouchers = Arc::new(InMemoryVoucherRepository::with_vouchers([voucher(10)]));
        let context = TransferContext::new(
            Arc::clone(&coins) as Arc<_>,
            Arc::clone(&vouchers) as Arc<_>,
        );
        context.reserve(&[1], &[10]).await.unwrap();
        context
            .process_outputs(&[1], &[10], &[coin(2)], &[coin(3)])
            .await
            .unwrap();

        assert_eq!(coin_state(&coins, 1).await, CoinState::Spent);
        assert_eq!(coin_state(&coins, 2).await, CoinState::Available);
        assert_eq!(coin_state(&coins, 3).await, CoinState::Spent);
        assert!(vouchers.list().await.unwrap().is_empty());
        context.revert().await.unwrap();
        assert_eq!(coin_state(&coins, 1).await, CoinState::Spent);
    }
}
