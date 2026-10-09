// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::collections::{HashMap, HashSet};
use {parking_lot::Mutex, std::sync::Arc};

use async_trait::async_trait;
use futures::future::AbortHandle;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::model::{Voucher, VoucherPrivacyLevel, VoucherRemoteState, ring_readiness_upgraded};
use crate::repo::VoucherRepository;

/// A member's on-chain ring position (`Members.Members` decode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingPosition {
    /// Membership submitted, no ring slot yet.
    Onboarding,
    /// Assigned to ring `ring_index` at slot `included_at`.
    Included { ring_index: u32, included_at: u32 },
}

/// A ring's key status (`Members.RingKeysStatus` decode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingStatus {
    /// How many member keys the ring has actually included.
    pub included_members: u32,
}

/// One partial emission from the batched subscription. Both vectors are
/// keyed by voucher derivation index; a `None` position means the member
/// entry disappeared.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VoucherLocationUpdate {
    pub ring_positions: Vec<(u32, Option<RingPosition>)>,
    pub ring_statuses: Vec<(u32, RingStatus)>,
}

/// The chain-subscription effect: ONE batched storage subscription
/// covering the three request families. Calling it again replaces the
/// previous subscription (the returned receiver of the old call simply
/// stops emitting once dropped).
#[async_trait]
pub trait VoucherLocationSubscriber: Send + Sync {
    /// `pending` — derivation indices still needing a location (not in a
    /// recycler); `included` — accumulated included positions as
    /// `(derivation_index, ring_index)`; `degraded` — degraded
    /// in-recycler vouchers as `(derivation_index, recycler_index)`.
    async fn subscribe(
        &self,
        pending: Vec<u32>,
        included: Vec<(u32, u32)>,
        degraded: Vec<(u32, u32)>,
    ) -> Result<mpsc::Receiver<VoucherLocationUpdate>, String>;
}

#[derive(Default)]
struct LocationState {
    /// The baseline: what the LIVE subscription covers.
    subscribed_indices: HashSet<u32>,
    /// Included positions seen so far (non-included ones are removed).
    positions: HashMap<u32, RingPosition>,
    /// Ring statuses seen so far.
    statuses: HashMap<u32, RingStatus>,
}

/// The service. `sync` (re)builds the subscription from current local
/// vouchers; call it at startup and after voucher saves. `stop` tears
/// the subscription down.
pub struct VoucherLocationService {
    spawner: crate::Spawner,
    vouchers: Arc<dyn VoucherRepository>,
    chain: Arc<dyn VoucherLocationSubscriber>,
    state: Mutex<LocationState>,
    task: Mutex<Option<AbortHandle>>,
}

impl VoucherLocationService {
    pub fn new(
        vouchers: Arc<dyn VoucherRepository>,
        chain: Arc<dyn VoucherLocationSubscriber>,
        spawner: crate::Spawner,
    ) -> Self {
        Self {
            vouchers,
            chain,
            state: Mutex::new(LocationState::default()),
            spawner,
            task: Mutex::new(None),
        }
    }

    /// (Re)builds the batched subscription: prune accumulated state to
    /// subscribe the three families. With nothing to watch, the live
    /// subscription is torn down.
    pub async fn sync(self: &Arc<Self>) -> Result<(), String> {
        let vouchers = self.vouchers.list().await?;
        let pending: Vec<u32> = vouchers
            .iter()
            .filter(|v| !v.remote_state.is_in_recycler())
            .map(|v| v.derivation_index)
            .collect();
        let degraded: Vec<(u32, u32)> = vouchers
            .iter()
            .filter_map(|v| match v.remote_state {
                VoucherRemoteState::InRecycler { recycler_index }
                    if v.privacy == VoucherPrivacyLevel::Degraded =>
                {
                    Some((v.derivation_index, recycler_index))
                }
                _ => None,
            })
            .collect();

        let included: Vec<(u32, u32)> = {
            let mut state = self.state.lock();
            let current: HashSet<u32> = pending.iter().copied().collect();
            state.positions.retain(|index, _| current.contains(index));
            state.statuses.retain(|index, _| current.contains(index));
            // The baseline reflects exactly what this batch subscribes.
            state.subscribed_indices = state.positions.keys().copied().collect();
            state
                .positions
                .iter()
                .filter_map(|(index, position)| match position {
                    RingPosition::Included { ring_index, .. } => Some((*index, *ring_index)),
                    RingPosition::Onboarding => None,
                })
                .collect()
        };

        if pending.is_empty() && degraded.is_empty() {
            self.stop();
            return Ok(());
        }

        let receiver = self.chain.subscribe(pending, included, degraded).await?;
        self.replace_task(receiver);
        Ok(())
    }

    /// Tears down the live subscription and resets accumulated state.
    pub fn stop(&self) {
        if let Some(handle) = self.task.lock().take() {
            handle.abort();
        }
        *self.state.lock() = LocationState::default();
    }

    fn replace_task(self: &Arc<Self>, mut receiver: mpsc::Receiver<VoucherLocationUpdate>) {
        let service = Arc::clone(self);
        let handle = crate::tasks::spawn_abortable(&self.spawner, async move {
            while let Some(update) = receiver.recv().await {
                match service.handle_update(update).await {
                    Ok(true) => {
                        let service = Arc::clone(&service);
                        crate::tasks::spawn_abortable(&service.spawner.clone(), async move {
                            if let Err(error) = service.sync().await {
                                warn!(error, "voucher location resubscription failed");
                            }
                        });
                        return;
                    }
                    Ok(false) => {}
                    Err(error) => warn!(error, "voucher location update failed"),
                }
            }
        });
        let previous = self.task.lock().replace(handle);
        if let Some(previous) = previous {
            previous.abort();
        }
    }

    /// One emission. Resolves `true` when a resubscription is needed
    /// (and the emission was deliberately not applied).
    async fn handle_update(&self, update: VoucherLocationUpdate) -> Result<bool, String> {
        let requires_resubscription = {
            let mut state = self.state.lock();
            for (index, position) in &update.ring_positions {
                match position {
                    Some(position @ RingPosition::Included { .. }) => {
                        state.positions.insert(*index, *position);
                    }
                    _ => {
                        state.positions.remove(index);
                    }
                }
            }
            for (index, status) in &update.ring_statuses {
                state.statuses.insert(*index, *status);
            }
            let accumulated: HashSet<u32> = state.positions.keys().copied().collect();
            accumulated != state.subscribed_indices
        };

        let voucher_map: HashMap<u32, Voucher> = self
            .vouchers
            .list()
            .await?
            .into_iter()
            .map(|v| (v.derivation_index, v))
            .collect();
        let mut updates: HashMap<u32, Voucher> = HashMap::new();

        let (positions, statuses) = {
            let state = self.state.lock();
            (state.positions.clone(), state.statuses.clone())
        };
        let waiting_for_status = positions
            .keys()
            .filter(|index| !statuses.contains_key(index))
            .count();
        let waiting_for_coverage = positions
            .iter()
            .filter(|(index, position)| {
                let RingPosition::Included { included_at, .. } = position else {
                    return false;
                };
                statuses
                    .get(index)
                    .is_some_and(|status| status.included_members <= *included_at)
            })
            .count();
        if requires_resubscription {
            log_reconciliation_summary(
                &voucher_map,
                positions.len(),
                waiting_for_status,
                waiting_for_coverage,
                0,
                0,
                true,
            );
            return Ok(true);
        }

        // Pass A — positions from THIS emission, statuses possibly
        // accumulated earlier.
        for (index, position) in &update.ring_positions {
            let Some(voucher) = updates
                .get(index)
                .cloned()
                .or_else(|| voucher_map.get(index).cloned())
            else {
                continue;
            };
            match position {
                Some(RingPosition::Included {
                    ring_index,
                    included_at,
                }) => {
                    // Deferred until the ring status covers the slot.
                    let Some(status) = statuses.get(index) else {
                        continue;
                    };
                    if status.included_members > *included_at {
                        updates.insert(
                            *index,
                            committed(voucher, *ring_index, status.included_members),
                        );
                    }
                }
                // Present but not included (or gone) → onboarding.
                _ => {
                    let mut voucher = voucher;
                    voucher.remote_state = VoucherRemoteState::Onboarding;
                    updates.insert(*index, voucher);
                }
            }
        }

        // Pass B — statuses from THIS emission, positions possibly
        // accumulated earlier; also upgrades degraded in-recycler
        for (index, status) in &update.ring_statuses {
            let Some(voucher) = updates
                .get(index)
                .cloned()
                .or_else(|| voucher_map.get(index).cloned())
            else {
                continue;
            };
            match (voucher.remote_state, positions.get(index)) {
                // Pending voucher whose position accumulated earlier.
                (
                    _,
                    Some(RingPosition::Included {
                        ring_index,
                        included_at,
                    }),
                ) if !voucher.remote_state.is_in_recycler() => {
                    if status.included_members > *included_at {
                        updates.insert(
                            *index,
                            committed(voucher, *ring_index, status.included_members),
                        );
                    }
                }
                // Degraded in-recycler voucher: readiness upgrade only.
                (VoucherRemoteState::InRecycler { .. }, _)
                    if voucher.privacy == VoucherPrivacyLevel::Degraded
                        && ring_readiness_upgraded(status.included_members) =>
                {
                    let mut voucher = voucher;
                    voucher.privacy = VoucherPrivacyLevel::Full;
                    updates.insert(*index, voucher);
                }
                _ => {}
            }
        }

        let mut location_updates = 0usize;
        let mut privacy_updates = 0usize;
        for (index, voucher) in &updates {
            // Only remote_state/privacy changed above; skip no-ops so a
            // repeated emission does not rewrite rows.
            let Some(previous) = voucher_map.get(index) else {
                continue;
            };
            if previous == voucher {
                continue;
            }
            if previous.privacy == voucher.privacy {
                self.vouchers
                    .set_remote_state(*index, voucher.remote_state)
                    .await?;
                location_updates += 1;
            } else {
                // Ring readiness changes privacy as well as location.
                self.vouchers.upsert(voucher).await?;
                if previous.remote_state != voucher.remote_state {
                    location_updates += 1;
                }
                if previous.privacy != voucher.privacy {
                    privacy_updates += 1;
                }
            }
        }
        log_reconciliation_summary(
            &voucher_map,
            positions.len(),
            waiting_for_status,
            waiting_for_coverage,
            location_updates,
            privacy_updates,
            false,
        );
        Ok(false)
    }
}

fn log_reconciliation_summary(
    vouchers: &HashMap<u32, Voucher>,
    tracked_positions: usize,
    waiting_for_status: usize,
    waiting_for_coverage: usize,
    location_changes: usize,
    privacy_changes: usize,
    resubscription_required: bool,
) {
    let tracked_total = vouchers.len();
    let in_recycler_total = vouchers
        .values()
        .filter(|voucher| voucher.remote_state.is_in_recycler())
        .count();
    let pending_location_total = tracked_total.saturating_sub(in_recycler_total);
    let degraded_total = vouchers
        .values()
        .filter(|voucher| voucher.privacy == VoucherPrivacyLevel::Degraded)
        .count();
    debug!(
        tracked_total,
        in_recycler_total,
        pending_location_total,
        degraded_total,
        waiting_for_status,
        waiting_for_coverage,
        location_changes,
        privacy_changes,
        tracked_positions,
        resubscription_required,
        changes_scope = "current_emission",
        "coinage voucher location emission reconciled"
    );
}

fn committed(mut voucher: Voucher, ring_index: u32, included_members: u32) -> Voucher {
    voucher.remote_state = VoucherRemoteState::InRecycler {
        recycler_index: ring_index,
    };
    if voucher.privacy == VoucherPrivacyLevel::Degraded && ring_readiness_upgraded(included_members)
    {
        voucher.privacy = VoucherPrivacyLevel::Full;
    }
    voucher
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::compute_balance;
    use crate::constants::MINIMUM_RING_SIZE;
    use crate::denomination::DenominationBreakdownContext;
    use crate::model::VoucherLocalState;
    use crate::repo::InMemoryVoucherRepository;

    /// One recorded `subscribe` call's arguments.
    type SubscribeCall = (Vec<u32>, Vec<(u32, u32)>, Vec<(u32, u32)>);

    /// Scripted subscriber: records each subscribe call's arguments and
    /// hands out a fresh channel per call.
    #[derive(Default)]
    struct MockSubscriber {
        calls: Mutex<Vec<SubscribeCall>>,
        senders: Mutex<Vec<mpsc::Sender<VoucherLocationUpdate>>>,
    }

    impl MockSubscriber {
        fn latest_sender(&self) -> mpsc::Sender<VoucherLocationUpdate> {
            self.senders
                .lock()
                .last()
                .expect("no subscription yet")
                .clone()
        }
    }

    #[async_trait]
    impl VoucherLocationSubscriber for MockSubscriber {
        async fn subscribe(
            &self,
            pending: Vec<u32>,
            included: Vec<(u32, u32)>,
            degraded: Vec<(u32, u32)>,
        ) -> Result<mpsc::Receiver<VoucherLocationUpdate>, String> {
            self.calls.lock().push((pending, included, degraded));
            let (tx, rx) = mpsc::channel(8);
            self.senders.lock().push(tx);
            Ok(rx)
        }
    }

    fn onboarding_voucher(index: u32) -> Voucher {
        Voucher {
            exponent: 1,
            derivation_index: index,
            allocated_at_ms: 0,
            ready_at_ms: 0,
            remote_state: VoucherRemoteState::Onboarding,
            local_state: VoucherLocalState::Available,
            privacy: VoucherPrivacyLevel::Full,
        }
    }

    fn degraded_in_recycler(index: u32, recycler: u32) -> Voucher {
        Voucher {
            remote_state: VoucherRemoteState::InRecycler {
                recycler_index: recycler,
            },
            privacy: VoucherPrivacyLevel::Degraded,
            ..onboarding_voucher(index)
        }
    }

    fn service(
        vouchers: Arc<InMemoryVoucherRepository>,
    ) -> (Arc<VoucherLocationService>, Arc<MockSubscriber>) {
        let subscriber = Arc::new(MockSubscriber::default());
        (
            Arc::new(VoucherLocationService::new(
                vouchers,
                Arc::clone(&subscriber) as Arc<dyn VoucherLocationSubscriber>,
                crate::test_spawner(),
            )),
            subscriber,
        )
    }

    async fn voucher_state(repo: &InMemoryVoucherRepository, index: u32) -> Voucher {
        repo.list()
            .await
            .unwrap()
            .into_iter()
            .find(|v| v.derivation_index == index)
            .unwrap()
    }

    async fn settle<T>(mut probe: impl AsyncFnMut() -> Option<T>) -> T {
        for _ in 0..1_000 {
            if let Some(value) = probe().await {
                return value;
            }
            tokio::task::yield_now().await;
        }
        panic!("never settled");
    }

    #[tokio::test]
    async fn two_phase_accumulation_commits_only_when_both_halves_agree() {
        let repo = Arc::new(InMemoryVoucherRepository::with_vouchers([
            onboarding_voucher(1),
        ]));
        let (service, subscriber) = service(Arc::clone(&repo));
        service.sync().await.unwrap();
        assert_eq!(subscriber.calls.lock().len(), 1);
        assert_eq!(
            subscriber.calls.lock()[0].0,
            vec![1],
            "pending voucher subscribed"
        );

        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![(
                    1,
                    Some(RingPosition::Included {
                        ring_index: 4,
                        included_at: 2,
                    }),
                )],
                ring_statuses: vec![],
            })
            .await
            .unwrap();
        settle(async || (subscriber.calls.lock().len() == 2).then_some(())).await;
        assert_eq!(
            subscriber.calls.lock()[1].1,
            vec![(1, 4)],
            "resubscription carries the accumulated included position"
        );
        assert_eq!(
            voucher_state(&repo, 1).await.remote_state,
            VoucherRemoteState::Onboarding,
            "no commit before the ring status arrives"
        );

        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![],
                ring_statuses: vec![(
                    1,
                    RingStatus {
                        included_members: 3,
                    },
                )],
            })
            .await
            .unwrap();
        let committed = settle(async || {
            let voucher = voucher_state(&repo, 1).await;
            voucher.remote_state.is_in_recycler().then_some(voucher)
        })
        .await;
        assert_eq!(
            committed.remote_state,
            VoucherRemoteState::InRecycler { recycler_index: 4 }
        );
    }

    /// Position and status arriving in the same emission still forces a
    /// resubscription, since the baseline only tracks previously-known
    /// included indices; the commit lands once the status is re-delivered
    /// on the new subscription.
    #[tokio::test]
    async fn position_and_status_in_one_emission_commit_after_resync() {
        let repo = Arc::new(InMemoryVoucherRepository::with_vouchers([
            onboarding_voucher(7),
        ]));
        let (service, subscriber) = service(Arc::clone(&repo));
        service.sync().await.unwrap();

        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![(
                    7,
                    Some(RingPosition::Included {
                        ring_index: 2,
                        included_at: 0,
                    }),
                )],
                ring_statuses: vec![(
                    7,
                    RingStatus {
                        included_members: 1,
                    },
                )],
            })
            .await
            .unwrap();
        // The first emission triggers resubscription; re-deliver the
        // status on the new batch (position stays accumulated).
        settle(async || (subscriber.calls.lock().len() == 2).then_some(())).await;
        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![],
                ring_statuses: vec![(
                    7,
                    RingStatus {
                        included_members: 1,
                    },
                )],
            })
            .await
            .unwrap();
        let committed = settle(async || {
            let voucher = voucher_state(&repo, 7).await;
            voucher.remote_state.is_in_recycler().then_some(voucher)
        })
        .await;
        assert_eq!(
            committed.remote_state,
            VoucherRemoteState::InRecycler { recycler_index: 2 }
        );
    }

    #[tokio::test]
    async fn ring_size_upgrade_boundary() {
        let repo = Arc::new(InMemoryVoucherRepository::with_vouchers([
            degraded_in_recycler(1, 9),
        ]));
        let (service, subscriber) = service(Arc::clone(&repo));
        service.sync().await.unwrap();
        assert_eq!(
            subscriber.calls.lock()[0].2,
            vec![(1, 9)],
            "degraded in-recycler voucher gets a ring-status request"
        );

        // One below the threshold: NOT upgraded.
        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![],
                ring_statuses: vec![(
                    1,
                    RingStatus {
                        included_members: MINIMUM_RING_SIZE - 1,
                    },
                )],
            })
            .await
            .unwrap();
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            voucher_state(&repo, 1).await.privacy,
            VoucherPrivacyLevel::Degraded
        );

        // Exactly the threshold: upgraded.
        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![],
                ring_statuses: vec![(
                    1,
                    RingStatus {
                        included_members: MINIMUM_RING_SIZE,
                    },
                )],
            })
            .await
            .unwrap();
        let upgraded = settle(async || {
            let voucher = voucher_state(&repo, 1).await;
            (voucher.privacy == VoucherPrivacyLevel::Full).then_some(voucher)
        })
        .await;
        assert!(upgraded.remote_state.is_in_recycler(), "location untouched");
    }

    #[test]
    fn minimum_ring_size_is_ten() {
        assert_eq!(MINIMUM_RING_SIZE, 10);
    }

    #[tokio::test]
    async fn stale_accumulated_state_is_pruned_on_sync() {
        let repo = Arc::new(InMemoryVoucherRepository::with_vouchers([
            onboarding_voucher(1),
            onboarding_voucher(2),
        ]));
        let (service, subscriber) = service(Arc::clone(&repo));
        service.sync().await.unwrap();
        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![(
                    2,
                    Some(RingPosition::Included {
                        ring_index: 1,
                        included_at: 0,
                    }),
                )],
                ring_statuses: vec![],
            })
            .await
            .unwrap();
        settle(async || (subscriber.calls.lock().len() == 2).then_some(())).await;

        // Voucher 2 disappears locally; the next sync prunes it.
        repo.remove(2).await.unwrap();
        service.sync().await.unwrap();
        let calls = subscriber.calls.lock();
        let last = calls.last().unwrap();
        assert_eq!(last.0, vec![1], "only the live voucher is pending");
        assert!(last.1.is_empty(), "stale accumulated position pruned");
    }

    /// A not-included member entry maps the voucher to `Onboarding`, and
    /// an all-quiet wallet tears the subscription down.
    #[tokio::test]
    async fn not_included_position_marks_onboarding() {
        let mut unlocated = onboarding_voucher(3);
        unlocated.remote_state = VoucherRemoteState::Unlocated;
        let repo = Arc::new(InMemoryVoucherRepository::with_vouchers([unlocated]));
        let (service, subscriber) = service(Arc::clone(&repo));
        service.sync().await.unwrap();
        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: vec![(3, Some(RingPosition::Onboarding))],
                ring_statuses: vec![],
            })
            .await
            .unwrap();
        let committed = settle(async || {
            let voucher = voucher_state(&repo, 3).await;
            (voucher.remote_state == VoucherRemoteState::Onboarding).then_some(())
        })
        .await;
        let () = committed;

        // Nothing left to watch once the voucher graduates fully.
        repo.remove(3).await.unwrap();
        service.sync().await.unwrap();
        assert!(service.task.lock().is_none(), "subscription torn down");
    }

    #[tokio::test]
    async fn six_vouchers_converge_across_reordered_partial_emissions() {
        let recyclers = [2, 2, 3, 4, 4, 5];
        let original = (1u32..=6)
            .map(|index| Voucher {
                exponent: i16::try_from(index - 1).unwrap(),
                derivation_index: index,
                allocated_at_ms: 1_000 + i64::from(index),
                ready_at_ms: 2_000 + i64::from(index),
                remote_state: VoucherRemoteState::Onboarding,
                local_state: VoucherLocalState::Available,
                privacy: VoucherPrivacyLevel::Full,
            })
            .collect::<Vec<_>>();
        let repo = Arc::new(InMemoryVoucherRepository::with_vouchers(original.clone()));
        let (service, subscriber) = service(Arc::clone(&repo));
        service.sync().await.unwrap();

        subscriber
            .latest_sender()
            .send(VoucherLocationUpdate {
                ring_positions: (1u32..=6)
                    .map(|index| {
                        (
                            index,
                            Some(RingPosition::Included {
                                ring_index: recyclers[usize::try_from(index - 1).unwrap()],
                                included_at: index - 1,
                            }),
                        )
                    })
                    .collect(),
                ring_statuses: Vec::new(),
            })
            .await
            .unwrap();
        settle(async || (subscriber.calls.lock().len() == 2).then_some(())).await;
        let mut included = subscriber.calls.lock()[1].1.clone();
        included.sort_unstable();
        assert_eq!(
            included,
            (1u32..=6)
                .map(|index| (index, recyclers[usize::try_from(index - 1).unwrap()]))
                .collect::<Vec<_>>(),
            "the replacement batch covers every newly included voucher"
        );

        for index in [6u32, 2, 5, 1, 4, 3] {
            subscriber
                .latest_sender()
                .send(VoucherLocationUpdate {
                    ring_positions: Vec::new(),
                    ring_statuses: vec![(
                        index,
                        RingStatus {
                            included_members: MINIMUM_RING_SIZE,
                        },
                    )],
                })
                .await
                .unwrap();
        }

        let converged = settle(async || {
            let rows = repo.list().await.unwrap();
            rows.iter()
                .all(|voucher| voucher.remote_state.is_in_recycler())
                .then_some(rows)
        })
        .await;
        for (before, after) in original.iter().zip(&converged) {
            assert_eq!(after.derivation_index, before.derivation_index);
            assert_eq!(after.exponent, before.exponent);
            assert_eq!(after.allocated_at_ms, before.allocated_at_ms);
            assert_eq!(after.ready_at_ms, before.ready_at_ms);
            assert_eq!(after.local_state, before.local_state);
            assert_eq!(after.privacy, before.privacy);
            assert_eq!(
                after.remote_state,
                VoucherRemoteState::InRecycler {
                    recycler_index: recyclers
                        [usize::try_from(before.derivation_index - 1).unwrap()]
                }
            );
        }

        let balance = compute_balance(
            &[],
            &converged,
            &DenominationBreakdownContext {
                asset_unit: 1,
                max_exponent: 10,
                min_exponent: 0,
                precision: 2,
            },
            10_000,
        );
        assert_eq!(balance.full_privacy_planks, 63);
        assert_eq!(balance.degraded_planks, 0);
    }
}
