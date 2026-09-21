// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer core/crates/brevity-coinage/src/backup.rs.
// Copyright the Brevity contributors. See truapi-coinage/NOTICE and LICENSE.

//! Native backup recovery with durable, head-pinned batch checkpoints.
//!
//! Preserve Brevity's 500-index batches, four consecutive empty batches,
//! inclusive saved-counter sweep, stable refresh horizon, and highest
//! *found* index counters (never the scan horizon). Host additions make each
//! batch's imported records, counters, and progress one atomic transaction.
//! An absent database is unscanned, not proof of an empty purse.

use std::sync::Arc;

use parity_scale_codec::{Decode, DecodeAll, Encode};
use truapi_coinage::{
    Coin, CoinKeypairFactory, CoinOnChainQueryService, CoinState, CoinageIndexStore,
    CoinageStorageQuery, IndexKind, RECYCLER_ALIAS_CONTEXT, Voucher, VoucherCryptography,
    VoucherKeypairFactory, VoucherLocalState, VoucherOnChainQueryService, VoucherPrivacyLevel,
    VoucherRemoteState, members::RingPosition,
};
use zeroize::Zeroizing;

use crate::runtime::{coinage_chain::HostCoinageChain, coinage_store::HostCoinageStore};

const SCAN_BATCH_SIZE: u32 = 500;
const SCAN_GAP_LIMIT: u32 = 4;
const PROGRESS_VERSION: u8 = 1;
const INDEX_SPACE_END: u64 = u32::MAX as u64 + 1;

/// Exclude this Host-private checkpoint from payment-operation enumeration.
pub(super) fn progress_id() -> [u8; 32] {
    sp_crypto_hashing::blake2_256(b"truapi/main-purse/coinage/inventory/v1")
}

/// Both repositories were fully scanned at this finalized snapshot. The caller
/// must still reconcile existing reservations and refresh live state before use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InventoryScanOutcome {
    pub coin_horizon: u32,
    pub voucher_horizon: u32,
    pub finalized_head: [u8; 32],
}

#[derive(Clone, Encode, Decode)]
struct ScanProgress {
    // Capture the original surviving counter before any recovered row raises it.
    // None means gap scan; it must not turn into a bounded scan on restart.
    through: Option<u32>,
    // Automatic discovery revisits the old frontier before applying the gap
    // stop. A quiet refresh does not march that frontier forward indefinitely.
    minimum_horizon: Option<u32>,
    // u64 represents exhaustion after scanning u32::MAX without wrapping/reuse.
    next_index: u64,
    empty_batches: u32,
    complete: bool,
}

impl ScanProgress {
    fn initial(through: Option<u32>) -> Self {
        Self {
            through,
            minimum_horizon: None,
            next_index: 0,
            empty_batches: 0,
            complete: false,
        }
    }

    fn refresh(minimum_horizon: Option<u32>) -> Self {
        Self {
            through: None,
            minimum_horizon,
            next_index: 0,
            empty_batches: 0,
            complete: false,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.next_index > INDEX_SPACE_END || self.empty_batches > SCAN_GAP_LIMIT {
            return Err("invalid Coinage inventory scan progress".into());
        }
        let done = match self.through {
            Some(through) => {
                let end = u64::from(through) + 1;
                if self.next_index > end
                    || self.empty_batches != 0
                    || self.minimum_horizon.is_some()
                {
                    return Err("invalid bounded Coinage inventory scan progress".into());
                }
                self.next_index == end
            }
            None => {
                (self.empty_batches == SCAN_GAP_LIMIT
                    && self
                        .minimum_horizon
                        .is_none_or(|minimum| self.next_index > u64::from(minimum)))
                    || self.next_index == INDEX_SPACE_END
            }
        };
        if self.complete != done || (self.complete && self.next_index == 0) {
            return Err("inconsistent Coinage inventory scan completion".into());
        }
        Ok(())
    }

    fn range(&self) -> Result<(u32, u32), String> {
        let start = u32::try_from(self.next_index)
            .map_err(|_| "Coinage inventory derivation space exhausted")?;
        let end = start.saturating_add(SCAN_BATCH_SIZE - 1);
        Ok((start, self.through.map_or(end, |through| end.min(through))))
    }

    fn advance(&mut self, end: u32, found: bool) {
        self.next_index = u64::from(end) + 1;
        self.complete = match self.through {
            Some(through) => end == through,
            None => {
                self.empty_batches = if found {
                    0
                } else {
                    (self.empty_batches + 1).min(SCAN_GAP_LIMIT)
                };
                (self.empty_batches == SCAN_GAP_LIMIT
                    && self.minimum_horizon.is_none_or(|minimum| end >= minimum))
                    || end == u32::MAX
            }
        };
    }

    fn horizon(&self) -> Result<u32, String> {
        if !self.complete {
            return Err("Coinage inventory scan is incomplete".into());
        }
        self.next_index
            .checked_sub(1)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| "invalid Coinage inventory scan horizon".into())
    }
}

#[derive(Clone, Encode, Decode)]
struct Progress {
    version: u8,
    finalized_head: [u8; 32],
    recovered_at_ms: i64,
    coins: ScanProgress,
    vouchers: ScanProgress,
}

impl Progress {
    fn restore(bytes: &[u8]) -> Result<Self, String> {
        let progress = Self::decode_all(&mut &bytes[..])
            .map_err(|_| "invalid Coinage inventory checkpoint")?;
        if progress.version != PROGRESS_VERSION {
            return Err("unsupported Coinage inventory checkpoint version".into());
        }
        progress.coins.validate()?;
        progress.vouchers.validate()?;
        Ok(progress)
    }

    fn complete(&self) -> bool {
        self.coins.complete && self.vouchers.complete
    }

    fn outcome(&self) -> Result<InventoryScanOutcome, String> {
        Ok(InventoryScanOutcome {
            coin_horizon: self.coins.horizon()?,
            voucher_horizon: self.vouchers.horizon()?,
            finalized_head: self.finalized_head,
        })
    }
}

struct QueryScan {
    coin_keys: CoinKeypairFactory,
    coins: CoinOnChainQueryService,
    vouchers: VoucherOnChainQueryService,
}

impl QueryScan {
    fn new(
        chain: Arc<HostCoinageChain>,
        entropy: Zeroizing<Vec<u8>>,
        crypto: Arc<dyn VoucherCryptography>,
    ) -> Self {
        let coin_keys = CoinKeypairFactory::new(&entropy);
        let voucher_keys = Arc::new(VoucherKeypairFactory::new(&entropy));
        // Factories own zeroize-on-drop key material; no entropy crosses RPC.
        drop(entropy);
        let public_key_provider = {
            let keys = voucher_keys.clone();
            let crypto = crypto.clone();
            Arc::new(move |index| keys.public_key(index, crypto.as_ref()))
        };
        let alias_provider = Arc::new(move |index| {
            voucher_keys.alias(index, RECYCLER_ALIAS_CONTEXT, crypto.as_ref())
        });
        Self {
            coin_keys,
            coins: CoinOnChainQueryService::new(chain.clone()),
            vouchers: VoucherOnChainQueryService::new(chain, public_key_provider, alias_provider),
        }
    }

    async fn coins(&self, start: u32, end: u32, at: [u8; 32]) -> Result<Vec<Coin>, String> {
        let public_keys = (start..=end)
            .map(|index| self.coin_keys.public_key(index))
            .collect::<Result<Vec<_>, _>>()?;
        let rows = self
            .coins
            .fetch_coins(&public_keys, Some(at))
            .await
            .map_err(|error| error.to_string())?;
        Ok((start..=end)
            .zip(rows)
            .filter_map(|(derivation_index, row)| {
                row.map(|coin| Coin {
                    exponent: coin.exponent,
                    derivation_index,
                    age: Some(coin.age),
                    state: CoinState::Available,
                })
            })
            .collect())
    }

    async fn vouchers(
        &self,
        start: u32,
        end: u32,
        at: [u8; 32],
        now_ms: i64,
    ) -> Result<(Vec<Voucher>, Option<u32>), String> {
        let indices = (start..=end).collect::<Vec<_>>();
        let rows = self
            .vouchers
            .fetch_recovery_vouchers(&indices, at)
            .await
            .map_err(|error| error.to_string())?;
        let mut vouchers = Vec::new();
        let mut highest = None;
        for (derivation_index, row) in indices.into_iter().zip(rows) {
            let Some(voucher) = row else { continue };
            // Even an unloaded voucher is a hit: it resets the gap and burns
            // its derivation index, but is never imported as spendable money.
            highest = Some(derivation_index);
            if voucher.is_unloaded {
                continue;
            }
            let remote_state = match voucher.ring_position {
                RingPosition::Included { ring_index, .. } => VoucherRemoteState::InRecycler {
                    recycler_index: ring_index,
                },
                RingPosition::Onboarding { .. } | RingPosition::Suspended => {
                    VoucherRemoteState::Onboarding
                }
            };
            vouchers.push(Voucher {
                exponent: voucher.exponent,
                derivation_index,
                allocated_at_ms: now_ms,
                ready_at_ms: 0,
                remote_state,
                local_state: VoucherLocalState::Available,
                privacy: VoucherPrivacyLevel::Degraded,
            });
        }
        Ok((vouchers, highest))
    }
}

/// Revisit every previously scanned or locally allocated index at a new
/// finalized head, then apply the native four-empty-batch stop. This catches
/// another device using an index inside an earlier empty gap. Only new hits
/// near the frontier extend it; passive refreshes do not add 2,000 indices.
/// Calls at an already completed finalized head reuse the existing outcome.

pub(super) async fn refresh_or_resume(
    store: Arc<HostCoinageStore>,
    chain: Arc<HostCoinageChain>,
    entropy: Zeroizing<Vec<u8>>,
    voucher_crypto: Arc<dyn VoucherCryptography>,
    now_ms: i64,
) -> Result<InventoryScanOutcome, String> {
    let id = progress_id();
    let mut expected = store
        .read_operation(id)
        .await
        .map_err(|error| error.to_string())?;
    let mut progress = match expected.as_deref() {
        Some(bytes) => {
            let previous = Progress::restore(bytes)?;
            if !previous.complete() {
                previous
            } else {
                let finalized_head = chain.finalized_head().await?;
                if finalized_head == previous.finalized_head {
                    return previous.outcome();
                }
                let (coin_index, voucher_index) = futures::try_join!(
                    store.current_index(IndexKind::Coin),
                    store.current_index(IndexKind::Voucher),
                )?;
                Progress {
                    version: PROGRESS_VERSION,
                    finalized_head,
                    recovered_at_ms: now_ms,
                    coins: ScanProgress::refresh(Some(
                        previous.coins.horizon()?.max(coin_index.unwrap_or(0)),
                    )),
                    vouchers: ScanProgress::refresh(Some(
                        previous.vouchers.horizon()?.max(voucher_index.unwrap_or(0)),
                    )),
                }
            }
        }
        None => {
            let (coins, vouchers, finalized_head) = futures::try_join!(
                store.current_index(IndexKind::Coin),
                store.current_index(IndexKind::Voucher),
                chain.finalized_head(),
            )?;
            Progress {
                version: PROGRESS_VERSION,
                finalized_head,
                recovered_at_ms: now_ms,
                coins: ScanProgress::initial(coins),
                vouchers: ScanProgress::initial(vouchers),
            }
        }
    };
    // Persist the initial plan before any batch can advance allocation counters.
    let encoded = progress.encode();
    if expected.as_ref() != Some(&encoded) {
        store
            .commit_inventory_batch(
                id,
                expected,
                encoded.clone(),
                Vec::new(),
                Vec::new(),
                None,
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        expected = Some(encoded);
    }
    let query = QueryScan::new(chain, entropy, voucher_crypto);
    while !progress.complete() {
        if !progress.coins.complete {
            let (start, end) = progress.coins.range()?;
            let coins = query.coins(start, end, progress.finalized_head).await?;
            let highest = coins.last().map(|coin| coin.derivation_index);
            progress.coins.advance(end, highest.is_some());
            let encoded = progress.encode();
            store
                .commit_inventory_batch(
                    id,
                    expected,
                    encoded.clone(),
                    coins,
                    Vec::new(),
                    highest,
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
            expected = Some(encoded);
        }
        if !progress.vouchers.complete {
            let (start, end) = progress.vouchers.range()?;
            let (vouchers, highest) = query
                .vouchers(
                    start,
                    end,
                    progress.finalized_head,
                    progress.recovered_at_ms,
                )
                .await?;
            progress.vouchers.advance(end, highest.is_some());
            let encoded = progress.encode();
            store
                .commit_inventory_batch(
                    id,
                    expected,
                    encoded.clone(),
                    Vec::new(),
                    vouchers,
                    None,
                    highest,
                )
                .await
                .map_err(|error| error.to_string())?;
            expected = Some(encoded);
        }
    }
    progress.outcome()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_gap_resets_on_used_evidence_and_survives_restart() {
        let mut progress = ScanProgress::initial(None);
        assert_eq!(progress.range().unwrap(), (0, 499));
        progress.advance(499, false);
        progress.advance(999, true);
        let mut restarted = ScanProgress::decode_all(&mut &progress.encode()[..]).unwrap();
        restarted.validate().unwrap();
        for end in [1499, 1999, 2499] {
            restarted.advance(end, false);
            assert!(!restarted.complete);
        }
        restarted.advance(2999, false);
        assert_eq!(restarted.horizon().unwrap(), 2999);
    }

    #[test]
    fn bounded_scan_is_inclusive_and_index_exhaustion_never_wraps() {
        let mut bounded = ScanProgress::initial(Some(500));
        assert_eq!(bounded.range().unwrap(), (0, 499));
        bounded.advance(499, false);
        assert_eq!(bounded.range().unwrap(), (500, 500));
        bounded.advance(500, true);
        assert_eq!(bounded.horizon().unwrap(), 500);
        let mut exhausted = ScanProgress {
            next_index: u64::from(u32::MAX),
            ..ScanProgress::refresh(None)
        };
        assert_eq!(exhausted.range().unwrap(), (u32::MAX, u32::MAX));
        exhausted.advance(u32::MAX, false);
        exhausted.validate().unwrap();
        assert_eq!(exhausted.horizon().unwrap(), u32::MAX);
        assert!(exhausted.range().is_err());
    }

    #[test]
    fn passive_refresh_revisits_old_gap_without_unbounded_frontier_growth() {
        let mut quiet = ScanProgress::refresh(Some(1999));
        for end in [499, 999, 1499, 1999] {
            quiet.advance(end, false);
        }
        assert_eq!(quiet.horizon().unwrap(), 1999);
        let mut changed = ScanProgress::refresh(Some(1999));
        changed.advance(499, false);
        changed.advance(999, false);
        changed.advance(1499, true);
        for end in [1999, 2499, 2999] {
            changed.advance(end, false);
            assert!(!changed.complete);
        }
        changed.advance(3499, false);
        assert_eq!(changed.horizon().unwrap(), 3499);
    }
}
