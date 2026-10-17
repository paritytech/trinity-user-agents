// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! Coin and voucher allocation: draws the next derivation index from the
//! [`CoinageIndexStore`] — the atomic read-increment-write counter — and
//! materializes the local record. A fresh voucher's `ready_at` is
//! `allocated_at + delay`, where the delay is drawn uniformly from
//! `0..=MAX_VOUCHER_WAIT_TIME` (6 h) to decorrelate onboarding batches; its
//! privacy level starts Degraded until the recycler ring proves large enough.

use std::sync::Arc;

use crate::clock::Clock;
use crate::constants::MAX_VOUCHER_WAIT_TIME;
use crate::index_store::{CoinageIndexStore, IndexKind};
use crate::model::{
    Coin, CoinState, Voucher, VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState,
};

/// The voucher readiness-delay source. Injected so tests pin it; the
/// production impl draws uniform jitter.
pub trait VoucherDelayProvider: Send + Sync {
    /// A delay in `0..=MAX_VOUCHER_WAIT_TIME` milliseconds.
    fn ready_delay_ms(&self) -> i64;
}

/// Production jitter from the OS entropy-backed hasher seed. Not
/// cryptographic — the delay only needs to be unpredictable enough to
/// decorrelate onboarding batches, matching the iOS
/// `TimeInterval.random(in: 0...maxVoucherWaitTime)`.
pub struct SystemJitterDelayProvider;

impl VoucherDelayProvider for SystemJitterDelayProvider {
    fn ready_delay_ms(&self) -> i64 {
        use std::hash::{BuildHasher, Hasher, RandomState};
        let raw = RandomState::new().build_hasher().finish();
        let span = MAX_VOUCHER_WAIT_TIME.as_millis() as u64 + 1;
        (raw % span) as i64
    }
}

/// Fixed delay for tests.
pub struct FixedDelayProvider(pub i64);

impl VoucherDelayProvider for FixedDelayProvider {
    fn ready_delay_ms(&self) -> i64 {
        self.0
    }
}

pub struct CoinAllocator {
    index_store: Arc<dyn CoinageIndexStore>,
}

impl CoinAllocator {
    pub fn new(index_store: Arc<dyn CoinageIndexStore>) -> Self {
        Self { index_store }
    }

    pub async fn allocate(&self, exponent: i16) -> Result<Coin, String> {
        let derivation_index = self.index_store.get_next_index(IndexKind::Coin).await?;
        Ok(Coin {
            exponent,
            derivation_index,
            age: None,
            state: CoinState::Available,
        })
    }
}

/// Allocates fresh vouchers: next index, `ready_at = now + jitter`,
/// `Unlocated` remote state and `Degraded` privacy until a later scan
/// reconciles the voucher's on-chain position.
pub struct VoucherAllocator {
    index_store: Arc<dyn CoinageIndexStore>,
    delay: Arc<dyn VoucherDelayProvider>,
    clock: Arc<dyn Clock>,
}

impl VoucherAllocator {
    pub fn new(
        index_store: Arc<dyn CoinageIndexStore>,
        delay: Arc<dyn VoucherDelayProvider>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            index_store,
            delay,
            clock,
        }
    }

    pub async fn allocate(&self, exponent: i16) -> Result<Voucher, String> {
        let derivation_index = self.index_store.get_next_index(IndexKind::Voucher).await?;
        let allocated_at_ms = self.clock.now_ms();
        Ok(Voucher {
            exponent,
            derivation_index,
            allocated_at_ms,
            ready_at_ms: allocated_at_ms.saturating_add(self.delay.ready_delay_ms()),
            remote_state: VoucherRemoteState::Unlocated,
            local_state: VoucherLocalState::Available,
            privacy: VoucherPrivacyLevel::Degraded,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::index_store::InMemoryCoinageIndexStore;

    #[tokio::test]
    async fn coin_allocation_draws_sequential_indices() {
        let store = Arc::new(InMemoryCoinageIndexStore::default());
        let allocator = CoinAllocator::new(Arc::clone(&store) as Arc<_>);
        let first = allocator.allocate(3).await.unwrap();
        let second = allocator.allocate(0).await.unwrap();
        assert_eq!(
            (first.derivation_index, second.derivation_index),
            (0, 1),
            "COINB-051 sequence"
        );
        assert_eq!(first.exponent, 3);
        assert_eq!(first.age, None, "age unknown until first sync");
        assert_eq!(first.state, CoinState::Available);
    }

    #[tokio::test]
    async fn voucher_allocation_applies_the_readiness_jitter() {
        let store = Arc::new(InMemoryCoinageIndexStore::default());
        let allocator = VoucherAllocator::new(
            Arc::clone(&store) as Arc<_>,
            Arc::new(FixedDelayProvider(5_000)),
            Arc::new(FixedClock(1_000)),
        );
        let voucher = allocator.allocate(2).await.unwrap();
        assert_eq!(voucher.derivation_index, 0);
        assert_eq!(voucher.allocated_at_ms, 1_000);
        assert_eq!(voucher.ready_at_ms, 6_000, "allocated_at + delay");
        assert_eq!(voucher.remote_state, VoucherRemoteState::Unlocated);
        assert_eq!(voucher.local_state, VoucherLocalState::Available);
        assert_eq!(
            voucher.privacy,
            VoucherPrivacyLevel::Degraded,
            "degraded until the ring proves large enough"
        );
    }

    /// Coin and voucher counters never share an index space.
    #[tokio::test]
    async fn allocators_use_independent_counters() {
        let store = Arc::new(InMemoryCoinageIndexStore::default());
        let coins = CoinAllocator::new(Arc::clone(&store) as Arc<_>);
        let vouchers = VoucherAllocator::new(
            Arc::clone(&store) as Arc<_>,
            Arc::new(FixedDelayProvider(0)),
            Arc::new(FixedClock(0)),
        );
        coins.allocate(0).await.unwrap();
        coins.allocate(0).await.unwrap();
        assert_eq!(vouchers.allocate(0).await.unwrap().derivation_index, 0);
    }

    #[test]
    fn system_jitter_stays_inside_the_wait_window() {
        let provider = SystemJitterDelayProvider;
        for _ in 0..64 {
            let delay = provider.ready_delay_ms();
            assert!(delay >= 0);
            assert!(delay <= MAX_VOUCHER_WAIT_TIME.as_millis() as i64);
        }
    }
}
