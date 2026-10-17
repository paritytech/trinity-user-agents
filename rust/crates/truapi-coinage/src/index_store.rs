// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use parking_lot::Mutex;
use std::collections::HashMap;

use async_trait::async_trait;
use parity_scale_codec::{Decode, Encode};

pub const COIN_INDEX_KEY: &str = "coin-index";

pub const VOUCHER_INDEX_KEY: &str = "voucher-index";

/// Which counter a call addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexKind {
    Coin,
    Voucher,
}

impl IndexKind {
    /// The platform storage key this counter lives under.
    pub fn storage_key(self) -> &'static str {
        match self {
            IndexKind::Coin => COIN_INDEX_KEY,
            IndexKind::Voucher => VOUCHER_INDEX_KEY,
        }
    }
}

pub fn encode_index(index: u32) -> Vec<u8> {
    index.encode()
}

/// Decode a stored counter; `None` on malformed bytes.
pub fn decode_index(bytes: &[u8]) -> Option<u32> {
    let mut input = bytes;
    let value = u32::decode(&mut input).ok()?;
    input.is_empty().then_some(value)
}

/// Durable, crash-safe monotonic counters. `get_next_index` is the
/// atomic read-increment-write allocation primitive: callers
/// (`CoinAllocator`/`VoucherAllocator`) receive each index exactly once.
#[async_trait]
pub trait CoinageIndexStore: Send + Sync {
    async fn get_next_index(&self, kind: IndexKind) -> Result<u32, String>;

    /// The current high-water mark; `None` on a fresh install.
    async fn current_index(&self, kind: IndexKind) -> Result<Option<u32>, String>;

    /// Overwrites the counter (used for backup-recovery horizon writes):
    /// only ever move it forward — a lower value re-issues already-used
    /// indices and corrupts key derivation.
    async fn set_index(&self, kind: IndexKind, index: u32) -> Result<(), String>;
}

/// Test/dev impl over a mutex-guarded map. Also the executable spec of
/// the counter contract for the platform impls.
#[derive(Default)]
pub struct InMemoryCoinageIndexStore {
    counters: Mutex<HashMap<IndexKind, u32>>,
}

#[async_trait]
impl CoinageIndexStore for InMemoryCoinageIndexStore {
    async fn get_next_index(&self, kind: IndexKind) -> Result<u32, String> {
        let mut counters = self.counters.lock();
        let next = match counters.get(&kind) {
            None => 0,
            Some(current) => current
                .checked_add(1)
                .ok_or_else(|| "derivation index space exhausted".to_string())?,
        };
        counters.insert(kind, next);
        Ok(next)
    }

    async fn current_index(&self, kind: IndexKind) -> Result<Option<u32>, String> {
        Ok(self.counters.lock().get(&kind).copied())
    }

    async fn set_index(&self, kind: IndexKind, index: u32) -> Result<(), String> {
        self.counters.lock().insert(kind, index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fresh_counter_starts_at_zero_then_increments() {
        let store = InMemoryCoinageIndexStore::default();
        assert_eq!(store.current_index(IndexKind::Coin).await.unwrap(), None);
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 0);
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 1);
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 2);
        assert_eq!(store.current_index(IndexKind::Coin).await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn coin_and_voucher_counters_are_independent() {
        let store = InMemoryCoinageIndexStore::default();
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 0);
        assert_eq!(store.get_next_index(IndexKind::Coin).await.unwrap(), 1);
        assert_eq!(store.get_next_index(IndexKind::Voucher).await.unwrap(), 0);
        assert_eq!(
            store.current_index(IndexKind::Voucher).await.unwrap(),
            Some(0)
        );
    }

    #[tokio::test]
    async fn horizon_write_moves_the_counter() {
        let store = InMemoryCoinageIndexStore::default();
        store.set_index(IndexKind::Voucher, 41).await.unwrap();
        assert_eq!(store.get_next_index(IndexKind::Voucher).await.unwrap(), 42);
    }

    /// Concurrent allocators must never observe the same index — the
    /// atomicity contract platform impls have to uphold.
    #[tokio::test]
    async fn concurrent_allocation_yields_unique_indices() {
        use std::collections::HashSet;
        use std::sync::Arc;
        let store = Arc::new(InMemoryCoinageIndexStore::default());
        let mut handles = Vec::new();
        for _ in 0..64 {
            let store = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                store.get_next_index(IndexKind::Coin).await.unwrap()
            }));
        }
        let mut seen = HashSet::new();
        for handle in handles {
            assert!(seen.insert(handle.await.unwrap()), "index issued twice");
        }
        assert_eq!(seen.len(), 64);
    }

    #[test]
    fn index_codec_is_scale_u32() {
        assert_eq!(encode_index(7), 7u32.encode());
        assert_eq!(decode_index(&encode_index(0xDEAD_BEEF)), Some(0xDEAD_BEEF));
        assert_eq!(decode_index(&[1, 2, 3]), None, "short read is malformed");
        assert_eq!(
            decode_index(&[1, 2, 3, 4, 5]),
            None,
            "trailing bytes are malformed"
        );
    }

    #[test]
    fn storage_keys_are_pinned() {
        assert_eq!(IndexKind::Coin.storage_key(), "coin-index");
        assert_eq!(IndexKind::Voucher.storage_key(), "voucher-index");
    }
}
