// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use crate::denomination::DenominationBreakdownContext;
use crate::model::{
    Coin, CoinState, EffectivePrivacy, Voucher, VoucherLocalState, VoucherPrivacyLevel,
};

/// The three balance buckets, in planks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BalanceBuckets {
    pub full_privacy_planks: u128,
    pub degraded_planks: u128,
    pub locked_planks: u128,
}

impl BalanceBuckets {
    pub fn total_planks(&self) -> u128 {
        self.full_privacy_planks
            .saturating_add(self.degraded_planks)
            .saturating_add(self.locked_planks)
    }
}

pub fn compute_balance(
    coins: &[Coin],
    vouchers: &[Voucher],
    context: &DenominationBreakdownContext,
    now_ms: i64,
) -> BalanceBuckets {
    let mut buckets = BalanceBuckets::default();
    for coin in coins {
        let value = context.value_in_planks(coin.exponent);
        match coin.state {
            CoinState::Available if coin.is_expiring_soon() => {
                buckets.locked_planks = buckets.locked_planks.saturating_add(value);
            }
            CoinState::Available => {
                buckets.full_privacy_planks = buckets.full_privacy_planks.saturating_add(value);
            }
            CoinState::Recycling => {
                buckets.locked_planks = buckets.locked_planks.saturating_add(value);
            }
            CoinState::PendingTransfer | CoinState::Spent => {}
        }
    }
    for voucher in vouchers {
        if voucher.local_state != VoucherLocalState::Available {
            continue;
        }
        let value = context.value_in_planks(voucher.exponent);
        if !voucher.remote_state.is_in_recycler() {
            buckets.locked_planks = buckets.locked_planks.saturating_add(value);
        } else {
            match voucher.effective_privacy(now_ms) {
                EffectivePrivacy::Full => {
                    buckets.full_privacy_planks = buckets.full_privacy_planks.saturating_add(value);
                }
                EffectivePrivacy::Degraded => {
                    buckets.degraded_planks = buckets.degraded_planks.saturating_add(value);
                }
            }
        }
    }
    buckets
}

pub fn next_unlock_at_ms(vouchers: &[Voucher], now_ms: i64) -> Option<i64> {
    vouchers
        .iter()
        .filter(|v| {
            v.privacy == VoucherPrivacyLevel::Full
                && v.local_state == VoucherLocalState::Available
                && v.remote_state.is_in_recycler()
                && v.ready_at_ms > now_ms
        })
        .map(|v| v.ready_at_ms)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VoucherRemoteState;

    fn ctx() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 4,
            min_exponent: 0,
            precision: 10,
        }
    }

    fn coin(index: u32, exponent: i16, age: Option<i16>, state: CoinState) -> Coin {
        Coin {
            exponent,
            derivation_index: index,
            age,
            state,
        }
    }

    fn voucher(
        index: u32,
        exponent: i16,
        remote: VoucherRemoteState,
        local: VoucherLocalState,
        privacy: VoucherPrivacyLevel,
        ready_at_ms: i64,
    ) -> Voucher {
        Voucher {
            exponent,
            derivation_index: index,
            allocated_at_ms: 0,
            ready_at_ms,
            remote_state: remote,
            local_state: local,
            privacy,
        }
    }

    const NOW: i64 = 10_000;
    const IN_RECYCLER: VoucherRemoteState = VoucherRemoteState::InRecycler { recycler_index: 0 };

    #[test]
    fn decomposition_with_known_coin_and_voucher_sets() {
        let coins = [
            coin(1, 0, Some(3), CoinState::Available),  // 10 → full
            coin(2, 1, None, CoinState::Available),     // 20 → full (unsynced age)
            coin(3, 2, Some(14), CoinState::Available), // 40 → locked (expiring)
            coin(4, 3, Some(2), CoinState::Recycling),  // 80 → locked
            coin(5, 4, Some(2), CoinState::PendingTransfer), // reserved → nowhere
            coin(6, 4, Some(2), CoinState::Spent),      // gone → nowhere
        ];
        let vouchers = [
            // 10 → full (ready, full privacy, in recycler).
            voucher(
                10,
                0,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                NOW,
            ),
            // 20 → degraded (full privacy but not ready yet).
            voucher(
                11,
                1,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                NOW + 1,
            ),
            // 40 → degraded (degraded at onboarding).
            voucher(
                12,
                2,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Degraded,
                0,
            ),
            // 80 → locked (not in a recycler yet).
            voucher(
                13,
                3,
                VoucherRemoteState::Onboarding,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                0,
            ),
            // reserved → nowhere.
            voucher(
                14,
                4,
                IN_RECYCLER,
                VoucherLocalState::PendingTransfer,
                VoucherPrivacyLevel::Full,
                0,
            ),
        ];
        let buckets = compute_balance(&coins, &vouchers, &ctx(), NOW);
        assert_eq!(buckets.full_privacy_planks, 10 + 20 + 10);
        assert_eq!(buckets.degraded_planks, 20 + 40);
        assert_eq!(buckets.locked_planks, 40 + 80 + 80);
        assert_eq!(buckets.total_planks(), 300);
    }

    #[test]
    fn empty_wallet_is_all_zero() {
        assert_eq!(
            compute_balance(&[], &[], &ctx(), NOW),
            BalanceBuckets::default()
        );
    }

    #[test]
    fn next_unlock_picks_the_earliest_upgradeable_voucher() {
        let vouchers = [
            voucher(
                1,
                0,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                NOW + 500,
            ),
            voucher(
                2,
                0,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                NOW + 100,
            ),
            // Already ready → no timer needed for it.
            voucher(
                3,
                0,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                NOW,
            ),
            // Degraded privacy never upgrades → ignored.
            voucher(
                4,
                0,
                IN_RECYCLER,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Degraded,
                NOW + 50,
            ),
            // Not in a recycler → ignored.
            voucher(
                5,
                0,
                VoucherRemoteState::Unlocated,
                VoucherLocalState::Available,
                VoucherPrivacyLevel::Full,
                NOW + 10,
            ),
        ];
        assert_eq!(next_unlock_at_ms(&vouchers, NOW), Some(NOW + 100));
        assert_eq!(next_unlock_at_ms(&vouchers[2..], NOW), None);
    }
}
