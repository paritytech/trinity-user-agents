// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use {parking_lot::Mutex, std::sync::Arc};

use async_trait::async_trait;
use futures::future::AbortHandle;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use crate::model::{Coin, CoinState, Voucher, VoucherLocalState};
use crate::repo::{CoinRepository, VoucherRepository};

/// One coin's on-chain reading (`CoinsByOwner` decode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnChainCoin {
    pub exponent: i16,
    pub age: i16,
}

/// One (possibly partial) emission: `None` means the coin's entry is
/// absent on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoinStateUpdate {
    pub coins: Vec<(u32, Option<OnChainCoin>)>,
}

/// The chain effect: ONE batched storage subscription over the given
/// derivation indices (key derivation happens inside the impl).
/// Re-calling replaces the previous subscription.
#[async_trait]
pub trait CoinStateSubscriber: Send + Sync {
    async fn subscribe(
        &self,
        derivation_indices: Vec<u32>,
    ) -> Result<mpsc::Receiver<CoinStateUpdate>, String>;
}

pub struct CoinStateSyncService {
    spawner: crate::Spawner,
    coins: Arc<dyn CoinRepository>,
    chain: Arc<dyn CoinStateSubscriber>,
    subscribed: Mutex<Vec<u32>>,
    task: Mutex<Option<AbortHandle>>,
}

impl CoinStateSyncService {
    pub fn new(
        coins: Arc<dyn CoinRepository>,
        chain: Arc<dyn CoinStateSubscriber>,
        spawner: crate::Spawner,
    ) -> Self {
        Self {
            coins,
            chain,
            subscribed: Mutex::new(Vec::new()),
            spawner,
            task: Mutex::new(None),
        }
    }

    /// (Re)builds the batch subscription over the monitored set. Call at
    /// startup and whenever local coins change; the service also resyncs
    /// itself when an applied update shrinks the monitored set.
    pub async fn sync(self: &Arc<Self>) -> Result<(), String> {
        let monitored = self.monitored_indices().await?;
        if monitored.is_empty() {
            self.stop();
            return Ok(());
        }
        *self.subscribed.lock() = monitored.clone();
        let receiver = self.chain.subscribe(monitored).await?;
        self.replace_task(receiver);
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(handle) = self.task.lock().take() {
            handle.abort();
        }
        self.subscribed.lock().clear();
    }

    async fn monitored_indices(&self) -> Result<Vec<u32>, String> {
        let mut indices: Vec<u32> = self
            .coins
            .list()
            .await?
            .into_iter()
            .filter(|c| c.state != CoinState::Spent && c.age.is_none())
            .map(|c| c.derivation_index)
            .collect();
        indices.sort_unstable();
        Ok(indices)
    }

    fn replace_task(self: &Arc<Self>, mut receiver: mpsc::Receiver<CoinStateUpdate>) {
        let service = Arc::clone(self);
        let handle = crate::tasks::spawn_abortable(&self.spawner, async move {
            while let Some(update) = receiver.recv().await {
                match service.handle_update(update).await {
                    Ok(true) => {
                        // The monitored set changed — rebuild.
                        let service = Arc::clone(&service);
                        crate::tasks::spawn_abortable(&service.spawner.clone(), async move {
                            if let Err(error) = service.sync().await {
                                warn!(error, "coin state resync failed");
                            }
                        });
                        return;
                    }
                    Ok(false) => {}
                    Err(error) => warn!(error, "coin state update failed"),
                }
            }
        });
        let previous = self.task.lock().replace(handle);
        if let Some(previous) = previous {
            previous.abort();
        }
    }

    /// Applies one emission; resolves `true` when the monitored set
    /// changed (resync needed).
    async fn handle_update(&self, update: CoinStateUpdate) -> Result<bool, String> {
        let coins = self.coins.list().await?;
        for (index, on_chain) in &update.coins {
            let Some(coin) = coins
                .iter()
                .find(|c| c.derivation_index == *index && c.state != CoinState::Spent)
            else {
                continue;
            };
            match on_chain {
                Some(reading) => {
                    if coin.age != Some(reading.age) {
                        let mut updated = coin.clone();
                        updated.age = Some(reading.age);
                        self.coins.upsert(&updated).await?;
                    }
                }
                None => {
                    if coin.age.is_some() {
                        self.coins
                            .set_state(coin.derivation_index, CoinState::Spent)
                            .await?;
                    }
                }
            }
        }
        let monitored = self.monitored_indices().await?;
        Ok(monitored != *self.subscribed.lock())
    }
}

/// [`CoinRepository`] decorator bumping a watch counter on every
/// successful mutation — the reactive change stream consumers re-fetch
/// on.
pub struct NotifyingCoinRepository {
    inner: Arc<dyn CoinRepository>,
    changes: watch::Sender<u64>,
}

impl NotifyingCoinRepository {
    pub fn new(inner: Arc<dyn CoinRepository>) -> Self {
        Self {
            inner,
            changes: watch::channel(0).0,
        }
    }

    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn bump(&self) {
        self.changes.send_modify(|generation| *generation += 1);
    }

    pub fn notify_changed(&self) {
        self.bump();
    }
}

#[async_trait]
impl CoinRepository for NotifyingCoinRepository {
    async fn list(&self) -> Result<Vec<Coin>, String> {
        self.inner.list().await
    }
    async fn upsert(&self, coin: &Coin) -> Result<(), String> {
        self.inner.upsert(coin).await?;
        self.bump();
        Ok(())
    }
    async fn set_state(&self, derivation_index: u32, state: CoinState) -> Result<(), String> {
        self.inner.set_state(derivation_index, state).await?;
        self.bump();
        Ok(())
    }
    async fn remove(&self, derivation_index: u32) -> Result<(), String> {
        self.inner.remove(derivation_index).await?;
        self.bump();
        Ok(())
    }
}

/// [`VoucherRepository`] decorator with the same change stream.
pub struct NotifyingVoucherRepository {
    inner: Arc<dyn VoucherRepository>,
    changes: watch::Sender<u64>,
}

impl NotifyingVoucherRepository {
    pub fn new(inner: Arc<dyn VoucherRepository>) -> Self {
        Self {
            inner,
            changes: watch::channel(0).0,
        }
    }

    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn bump(&self) {
        self.changes.send_modify(|generation| *generation += 1);
    }

    pub fn notify_changed(&self) {
        self.bump();
    }
}

#[async_trait]
impl VoucherRepository for NotifyingVoucherRepository {
    async fn list(&self) -> Result<Vec<Voucher>, String> {
        self.inner.list().await
    }
    async fn upsert(&self, voucher: &Voucher) -> Result<(), String> {
        self.inner.upsert(voucher).await?;
        self.bump();
        Ok(())
    }
    async fn set_local_state(
        &self,
        derivation_index: u32,
        state: VoucherLocalState,
    ) -> Result<(), String> {
        self.inner.set_local_state(derivation_index, state).await?;
        self.bump();
        Ok(())
    }
    async fn set_remote_state(
        &self,
        derivation_index: u32,
        state: crate::model::VoucherRemoteState,
    ) -> Result<(), String> {
        self.inner.set_remote_state(derivation_index, state).await?;
        self.bump();
        Ok(())
    }
    async fn remove(&self, derivation_index: u32) -> Result<(), String> {
        self.inner.remove(derivation_index).await?;
        self.bump();
        Ok(())
    }
}

pub struct CoinageDatabaseDependencyFactory {
    coins: Arc<NotifyingCoinRepository>,
    vouchers: Arc<NotifyingVoucherRepository>,
}

impl CoinageDatabaseDependencyFactory {
    pub fn new(coins: Arc<dyn CoinRepository>, vouchers: Arc<dyn VoucherRepository>) -> Self {
        Self {
            coins: Arc::new(NotifyingCoinRepository::new(coins)),
            vouchers: Arc::new(NotifyingVoucherRepository::new(vouchers)),
        }
    }

    pub fn coin_repository(&self) -> Arc<NotifyingCoinRepository> {
        Arc::clone(&self.coins)
    }

    pub fn voucher_repository(&self) -> Arc<NotifyingVoucherRepository> {
        Arc::clone(&self.vouchers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{InMemoryCoinRepository, InMemoryVoucherRepository};

    #[derive(Default)]
    struct MockSubscriber {
        calls: Mutex<Vec<Vec<u32>>>,
        senders: Mutex<Vec<mpsc::Sender<CoinStateUpdate>>>,
    }

    impl MockSubscriber {
        fn latest_sender(&self) -> mpsc::Sender<CoinStateUpdate> {
            self.senders.lock().last().expect("no subscription").clone()
        }
    }

    #[async_trait]
    impl CoinStateSubscriber for MockSubscriber {
        async fn subscribe(
            &self,
            derivation_indices: Vec<u32>,
        ) -> Result<mpsc::Receiver<CoinStateUpdate>, String> {
            self.calls.lock().push(derivation_indices);
            let (tx, rx) = mpsc::channel(8);
            self.senders.lock().push(tx);
            Ok(rx)
        }
    }

    fn coin(index: u32, age: Option<i16>, state: CoinState) -> Coin {
        Coin {
            exponent: 1,
            derivation_index: index,
            age,
            state,
        }
    }

    fn rig(
        coins: Vec<Coin>,
    ) -> (
        Arc<CoinStateSyncService>,
        Arc<InMemoryCoinRepository>,
        Arc<MockSubscriber>,
    ) {
        let repo = Arc::new(InMemoryCoinRepository::with_coins(coins));
        let subscriber = Arc::new(MockSubscriber::default());
        (
            Arc::new(CoinStateSyncService::new(
                Arc::clone(&repo) as Arc<dyn CoinRepository>,
                Arc::clone(&subscriber) as Arc<dyn CoinStateSubscriber>,
                crate::test_spawner(),
            )),
            repo,
            subscriber,
        )
    }

    async fn read_coin(repo: &InMemoryCoinRepository, index: u32) -> Coin {
        repo.list()
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.derivation_index == index)
            .unwrap()
    }

    async fn settle(mut probe: impl AsyncFnMut() -> bool) {
        for _ in 0..1_000 {
            if probe().await {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("never settled");
    }

    #[tokio::test]
    async fn age_lands_and_the_monitored_set_shrinks() {
        let (service, repo, subscriber) = rig(vec![
            coin(1, None, CoinState::Available),
            coin(2, Some(4), CoinState::Available), // known age — not monitored
            coin(3, None, CoinState::Spent),        // spent — not monitored
        ]);
        service.sync().await.unwrap();
        assert_eq!(subscriber.calls.lock().as_slice(), [vec![1]]);

        subscriber
            .latest_sender()
            .send(CoinStateUpdate {
                coins: vec![(
                    1,
                    Some(OnChainCoin {
                        exponent: 1,
                        age: 0,
                    }),
                )],
            })
            .await
            .unwrap();
        settle(async || read_coin(&repo, 1).await.age == Some(0)).await;
        // Monitored set now empty → the resync stops the subscription.
        settle(async || service.task.lock().is_none()).await;
    }

    #[tokio::test]
    async fn absence_spends_only_previously_observed_coins() {
        let (service, repo, subscriber) = rig(vec![
            coin(1, None, CoinState::Available),
            coin(2, None, CoinState::Available),
        ]);
        service.sync().await.unwrap();

        // Coin 1 lands with an age…
        subscriber
            .latest_sender()
            .send(CoinStateUpdate {
                coins: vec![(
                    1,
                    Some(OnChainCoin {
                        exponent: 1,
                        age: 2,
                    }),
                )],
            })
            .await
            .unwrap();
        settle(async || read_coin(&repo, 1).await.age == Some(2)).await;
        settle(async || subscriber.calls.lock().len() == 2).await;

        // …then disappears (claimed by a recipient) while coin 2 is
        // still absent because it never landed.
        subscriber
            .latest_sender()
            .send(CoinStateUpdate {
                coins: vec![(1, None), (2, None)],
            })
            .await
            .unwrap();
        settle(async || read_coin(&repo, 1).await.state == CoinState::Spent).await;
        let unlanded = read_coin(&repo, 2).await;
        assert_eq!(
            unlanded.state,
            CoinState::Available,
            "unknown-age absence is 'not landed yet', never 'spent'"
        );
        assert_eq!(unlanded.age, None);
    }

    #[tokio::test]
    async fn empty_monitored_set_never_subscribes() {
        let (service, _repo, subscriber) = rig(vec![coin(1, Some(3), CoinState::Available)]);
        service.sync().await.unwrap();
        assert!(subscriber.calls.lock().is_empty());
        assert!(service.task.lock().is_none());
    }

    #[tokio::test]
    async fn factory_change_streams_fire_on_mutation() {
        let factory = CoinageDatabaseDependencyFactory::new(
            Arc::new(InMemoryCoinRepository::default()),
            Arc::new(InMemoryVoucherRepository::default()),
        );
        let coins = factory.coin_repository();
        let mut coin_changes = coins.subscribe_changes();
        coins
            .upsert(&coin(1, None, CoinState::Available))
            .await
            .unwrap();
        coin_changes.changed().await.unwrap();

        let same = factory.coin_repository();
        assert_eq!(
            same.list().await.unwrap().len(),
            1,
            "same underlying instance"
        );

        let vouchers = factory.voucher_repository();
        let mut voucher_changes = vouchers.subscribe_changes();
        vouchers
            .upsert(&Voucher {
                exponent: 0,
                derivation_index: 1,
                allocated_at_ms: 0,
                ready_at_ms: 0,
                remote_state: crate::model::VoucherRemoteState::Unlocated,
                local_state: VoucherLocalState::Available,
                privacy: crate::model::VoucherPrivacyLevel::Degraded,
            })
            .await
            .unwrap();
        voucher_changes.changed().await.unwrap();
        vouchers
            .set_local_state(1, VoucherLocalState::Spent)
            .await
            .unwrap();
        voucher_changes.changed().await.unwrap();
    }
}
