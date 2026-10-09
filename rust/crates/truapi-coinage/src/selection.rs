// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use crate::denomination::{Denomination, DenominationBreakdownContext};
use crate::model::{Coin, EffectivePrivacy, Voucher, VoucherRemoteState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyLevel {
    Full,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoinSelectionError {
    ZeroAmount,
    EmptyWallet,
    /// The amount does not decompose exactly into denominations at this
    /// context's granularity (below `min_exponent`).
    AmountNotRepresentable {
        remainder: u128,
    },
    /// Vouchers exist but none are ready — distinct from
    /// `InsufficientFunds` so the UI can show "wait for maturity"
    NoReadyVouchers,
    TooManyVouchersInGroup {
        count: usize,
        max: usize,
    },
    InsufficientFunds,
}

impl std::fmt::Display for CoinSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoinSelectionError::ZeroAmount => write!(f, "amount must be greater than zero"),
            CoinSelectionError::EmptyWallet => write!(f, "no coins or vouchers available"),
            CoinSelectionError::AmountNotRepresentable { remainder } => {
                write!(
                    f,
                    "amount not representable ({remainder} planks below the smallest denomination)"
                )
            }
            CoinSelectionError::NoReadyVouchers => write!(f, "vouchers are not ready yet"),
            CoinSelectionError::TooManyVouchersInGroup { count, max } => {
                write!(f, "recycler group holds {count} vouchers, maximum {max}")
            }
            CoinSelectionError::InsufficientFunds => write!(f, "insufficient funds"),
        }
    }
}

impl std::error::Error for CoinSelectionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecyclerKey {
    pub exponent: i16,
    pub index: u32,
}

/// One unload group: all selected vouchers of one recycler, with the
/// group's output split into recipient + change denominations whose
/// combined value always equals the group's total voucher input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoucherGroup {
    pub recycler: RecyclerKey,
    pub vouchers: Vec<Voucher>,
    pub recipient_denominations: Vec<Denomination>,
    pub change_denominations: Vec<Denomination>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferStrategy {
    /// Whole coins pass to the recipient via the memo; nothing on-chain.
    ExactMatch { coins: Vec<Coin> },
    /// `partial_coins` pass whole; `split_coin` is split into
    /// `target_denominations` (recipient) + `change_denominations`
    /// (sender), signed with the coin's own key (AsCoin origin).
    Split {
        partial_coins: Vec<Coin>,
        split_coin: Coin,
        target_denominations: Vec<Denomination>,
        change_denominations: Vec<Denomination>,
    },
    /// `coins` pass whole; each group is one
    /// `UnloadRecyclerIntoCoins` extrinsic (Ring-VRF origin).
    UnloadIntoCoins {
        coins: Vec<Coin>,
        groups: Vec<VoucherGroup>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoinSelectionResult {
    pub strategy: TransferStrategy,
    pub privacy_level: PrivacyLevel,
}

pub fn find_exact_match(
    amount: u128,
    coins: &[Coin],
    context: &DenominationBreakdownContext,
) -> Option<Vec<Coin>> {
    let (selected, remaining) = greedy_take(amount, coins, context);
    (remaining == 0).then_some(selected)
}

/// Greedy largest-first pass: takes coins whose value fits the remaining
/// amount; returns the take and what is left uncovered.
fn greedy_take(
    amount: u128,
    coins: &[Coin],
    context: &DenominationBreakdownContext,
) -> (Vec<Coin>, u128) {
    let mut pool: Vec<&Coin> = coins.iter().filter(|c| c.is_selectable()).collect();
    pool.sort_by(|a, b| {
        b.exponent
            .cmp(&a.exponent)
            .then(b.age.unwrap_or(-1).cmp(&a.age.unwrap_or(-1)))
            .then(a.derivation_index.cmp(&b.derivation_index))
    });
    let mut remaining = amount;
    let mut selected = Vec::new();
    for coin in pool {
        let value = context.value_in_planks(coin.exponent);
        if value <= remaining {
            remaining -= value;
            selected.push(coin.clone());
        }
    }
    (selected, remaining)
}

pub struct CoinSelector {
    context: DenominationBreakdownContext,
    max_consolidation: usize,
}

impl CoinSelector {
    pub fn new(context: DenominationBreakdownContext, max_consolidation: usize) -> Self {
        Self {
            context,
            max_consolidation,
        }
    }

    /// Runs the three strategies in priority order. `now_ms` drives
    /// voucher effective-privacy evaluation.
    pub fn select(
        &self,
        amount: u128,
        coins: &[Coin],
        vouchers: &[Voucher],
        now_ms: i64,
    ) -> Result<CoinSelectionResult, CoinSelectionError> {
        if amount == 0 {
            return Err(CoinSelectionError::ZeroAmount);
        }
        let unloadable: Vec<&Voucher> = vouchers.iter().filter(|v| v.is_unloadable()).collect();
        let any_available_coin = coins
            .iter()
            .any(|c| c.state == crate::model::CoinState::Available);
        if !any_available_coin && unloadable.is_empty() {
            return Err(CoinSelectionError::EmptyWallet);
        }
        let amount_breakdown = self.context.breakdown(amount);
        if !amount_breakdown.is_exact() {
            return Err(CoinSelectionError::AmountNotRepresentable {
                remainder: amount_breakdown.remainder,
            });
        }

        if let Some(selected) = find_exact_match(amount, coins, &self.context) {
            return Ok(CoinSelectionResult {
                strategy: TransferStrategy::ExactMatch { coins: selected },
                privacy_level: PrivacyLevel::Full,
            });
        }

        if let Some(result) = self.try_split_coin(amount, coins) {
            return Ok(CoinSelectionResult {
                strategy: result,
                privacy_level: PrivacyLevel::Full,
            });
        }

        let full_pool: Vec<&Voucher> = unloadable
            .iter()
            .copied()
            .filter(|v| v.privacy == crate::model::VoucherPrivacyLevel::Full)
            .collect();
        if let Some(strategy) = self.try_unload(amount, coins, &full_pool)? {
            let privacy_level = strategy_privacy(&strategy, now_ms);
            return Ok(CoinSelectionResult {
                strategy,
                privacy_level,
            });
        }

        if unloadable.len() > full_pool.len()
            && let Some(strategy) = self.try_unload(amount, coins, &unloadable)?
        {
            let privacy_level = strategy_privacy(&strategy, now_ms);
            return Ok(CoinSelectionResult {
                strategy,
                privacy_level,
            });
        }

        if !unloadable.is_empty() && full_pool.is_empty() {
            return Err(CoinSelectionError::NoReadyVouchers);
        }
        Err(CoinSelectionError::InsufficientFunds)
    }

    fn try_split_coin(&self, amount: u128, coins: &[Coin]) -> Option<TransferStrategy> {
        let mut selectable = coins
            .iter()
            .filter(|c| c.is_selectable())
            .cloned()
            .collect::<Vec<_>>();
        let sufficient = selectable
            .iter()
            .filter(|coin| self.context.value_in_planks(coin.exponent) > amount)
            .min_by_key(|coin| {
                (
                    self.context.value_in_planks(coin.exponent),
                    coin.derivation_index,
                )
            })
            .cloned();
        if let Some(split_coin) = sufficient {
            return Some(self.split_strategy(Vec::new(), split_coin, amount));
        }

        selectable.sort_by(|a, b| {
            b.exponent
                .cmp(&a.exponent)
                .then(a.derivation_index.cmp(&b.derivation_index))
        });
        let mut partial = Vec::new();
        let mut accumulated = 0u128;
        for coin in selectable {
            let value = self.context.value_in_planks(coin.exponent);
            let next = accumulated.saturating_add(value);
            if next < amount {
                partial.push(coin);
                accumulated = next;
                continue;
            }
            return Some(self.split_strategy(partial, coin, amount - accumulated));
        }
        None
    }

    fn split_strategy(
        &self,
        partial: Vec<Coin>,
        split_coin: Coin,
        remaining: u128,
    ) -> TransferStrategy {
        let coin_value = self.context.value_in_planks(split_coin.exponent);
        let target = self.context.breakdown(remaining);
        let change = self.context.breakdown(coin_value - remaining);
        debug_assert!(
            target.is_exact() && change.is_exact(),
            "powers of two split exactly"
        );
        TransferStrategy::Split {
            partial_coins: partial,
            split_coin,
            target_denominations: target.denominations,
            change_denominations: change.denominations,
        }
    }

    fn try_unload(
        &self,
        amount: u128,
        coins: &[Coin],
        pool: &[&Voucher],
    ) -> Result<Option<TransferStrategy>, CoinSelectionError> {
        if pool.is_empty() {
            return Ok(None);
        }
        let (partial, needed) = greedy_take(amount, coins, &self.context);
        debug_assert!(needed > 0);

        let chosen: Vec<&Voucher> = match pool
            .iter()
            .filter(|v| self.context.value_in_planks(v.exponent) >= needed)
            .min_by_key(|v| (self.context.value_in_planks(v.exponent), v.derivation_index))
        {
            Some(single) => vec![single],
            None => {
                // Greedy largest-first accumulation until covered.
                let mut sorted: Vec<&Voucher> = pool.to_vec();
                sorted.sort_by(|a, b| {
                    b.exponent
                        .cmp(&a.exponent)
                        .then(a.derivation_index.cmp(&b.derivation_index))
                });
                let mut sum = 0u128;
                let mut chosen = Vec::new();
                for voucher in sorted {
                    if sum >= needed {
                        break;
                    }
                    sum = sum.saturating_add(self.context.value_in_planks(voucher.exponent));
                    chosen.push(voucher);
                }
                if sum < needed {
                    return Ok(None);
                }
                chosen
            }
        };

        let mut groups: Vec<(RecyclerKey, Vec<Voucher>)> = Vec::new();
        for voucher in chosen {
            let VoucherRemoteState::InRecycler { recycler_index } = voucher.remote_state else {
                // is_unloadable filtered already; defensive.
                continue;
            };
            let key = RecyclerKey {
                exponent: voucher.exponent,
                index: recycler_index,
            };
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, members)) => members.push(voucher.clone()),
                None => groups.push((key, vec![voucher.clone()])),
            }
        }

        groups.sort_by(|(left, _), (right, _)| {
            right
                .exponent
                .cmp(&left.exponent)
                .then(left.index.cmp(&right.index))
        });

        let mut needed_left = needed;
        let mut allocated = Vec::with_capacity(groups.len());
        for (recycler, members) in groups {
            if members.len() >= self.max_consolidation {
                return Err(CoinSelectionError::TooManyVouchersInGroup {
                    count: members.len(),
                    max: self.max_consolidation,
                });
            }
            let input: u128 = members.iter().fold(0u128, |acc, v| {
                acc.saturating_add(self.context.value_in_planks(v.exponent))
            });
            let recipient_amount = needed_left.min(input);
            needed_left -= recipient_amount;
            let recipient = self.context.breakdown(recipient_amount);
            let change = self.context.breakdown(input - recipient_amount);
            debug_assert!(recipient.is_exact() && change.is_exact());
            allocated.push(VoucherGroup {
                recycler,
                vouchers: members,
                recipient_denominations: recipient.denominations,
                change_denominations: change.denominations,
            });
        }
        debug_assert_eq!(needed_left, 0);
        Ok(Some(TransferStrategy::UnloadIntoCoins {
            coins: partial,
            groups: allocated,
        }))
    }
}

fn strategy_privacy(strategy: &TransferStrategy, now_ms: i64) -> PrivacyLevel {
    match strategy {
        TransferStrategy::UnloadIntoCoins { groups, .. }
            if groups
                .iter()
                .flat_map(|group| &group.vouchers)
                .any(|voucher| voucher.effective_privacy(now_ms) == EffectivePrivacy::Degraded) =>
        {
            PrivacyLevel::Degraded
        }
        _ => PrivacyLevel::Full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CoinState, VoucherLocalState, VoucherPrivacyLevel};

    /// Unit 10, exponents 0..=4 → coin values 10..160.
    fn ctx() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 4,
            min_exponent: 0,
            precision: 10,
        }
    }

    fn selector() -> CoinSelector {
        CoinSelector::new(ctx(), 8)
    }

    fn coin(index: u32, exponent: i16, age: Option<i16>) -> Coin {
        Coin {
            exponent,
            derivation_index: index,
            age,
            state: CoinState::Available,
        }
    }

    fn voucher(
        index: u32,
        exponent: i16,
        recycler: u32,
        privacy: VoucherPrivacyLevel,
        ready_at_ms: i64,
    ) -> Voucher {
        Voucher {
            exponent,
            derivation_index: index,
            allocated_at_ms: 0,
            ready_at_ms,
            remote_state: VoucherRemoteState::InRecycler {
                recycler_index: recycler,
            },
            local_state: VoucherLocalState::Available,
            privacy,
        }
    }

    fn full_voucher(index: u32, exponent: i16) -> Voucher {
        voucher(index, exponent, 0, VoucherPrivacyLevel::Full, 0)
    }

    const NOW: i64 = 1_000_000;

    #[test]
    fn zero_amount_is_rejected() {
        assert_eq!(
            selector().select(0, &[coin(1, 0, None)], &[], NOW),
            Err(CoinSelectionError::ZeroAmount)
        );
    }

    #[test]
    fn empty_wallet_is_rejected() {
        assert_eq!(
            selector().select(10, &[], &[], NOW),
            Err(CoinSelectionError::EmptyWallet)
        );
        // A wallet of only spent coins is empty too.
        let spent = Coin {
            state: CoinState::Spent,
            ..coin(1, 4, None)
        };
        assert_eq!(
            selector().select(10, &[spent], &[], NOW),
            Err(CoinSelectionError::EmptyWallet)
        );
    }

    #[test]
    fn sub_denomination_amount_is_rejected() {
        assert_eq!(
            selector().select(15, &[coin(1, 4, None)], &[], NOW),
            Err(CoinSelectionError::AmountNotRepresentable { remainder: 5 })
        );
    }

    #[test]
    fn exact_match_selects_minimum_coins_largest_first() {
        let coins = [
            coin(1, 0, None),
            coin(2, 1, None),
            coin(3, 2, None),
            coin(4, 4, None),
        ];
        // 230 = 160 + 40 + 20 + 10 — all four; 200 = 160 + 40.
        let result = selector().select(200, &coins, &[], NOW).unwrap();
        let TransferStrategy::ExactMatch { coins: selected } = result.strategy else {
            panic!("expected exact match");
        };
        assert_eq!(
            selected
                .iter()
                .map(|c| c.derivation_index)
                .collect::<Vec<_>>(),
            [4, 3]
        );
        assert_eq!(result.privacy_level, PrivacyLevel::Full);
    }

    #[test]
    fn exact_match_prefers_older_coins_within_a_denomination() {
        let coins = [coin(1, 0, Some(2)), coin(2, 0, Some(9)), coin(3, 0, None)];
        let result = selector().select(10, &coins, &[], NOW).unwrap();
        let TransferStrategy::ExactMatch { coins: selected } = result.strategy else {
            panic!("expected exact match");
        };
        assert_eq!(selected[0].derivation_index, 2, "age 9 spends before age 2");
    }

    #[test]
    fn expiring_coins_never_enter_selection() {
        let coins = [coin(1, 0, Some(14)), coin(2, 0, Some(13))];
        // 20 would need both — the expiring one is invisible.
        assert_eq!(
            selector().select(20, &coins, &[], NOW),
            Err(CoinSelectionError::InsufficientFunds)
        );
        // 10 matches with the young coin only.
        let result = selector().select(10, &coins, &[], NOW).unwrap();
        let TransferStrategy::ExactMatch { coins: selected } = result.strategy else {
            panic!("expected exact match");
        };
        assert_eq!(selected[0].derivation_index, 2);
    }

    #[test]
    fn find_exact_match_returns_none_when_no_exact_subset_exists() {
        assert!(find_exact_match(30, &[coin(1, 2, None)], &ctx()).is_none());
        assert!(find_exact_match(30, &[coin(1, 1, None), coin(2, 0, None)], &ctx()).is_some());
    }

    #[test]
    fn split_picks_smallest_coin_larger_than_the_remainder() {
        // Amount 30: no exact match from {160, 80}; split the 80? No —
        // smallest coin larger than 30 is 80 (overshoot 50) vs 160
        // (overshoot 130) → split coin 2 (exp 3).
        let coins = [coin(1, 4, None), coin(2, 3, None)];
        let result = selector().select(30, &coins, &[], NOW).unwrap();
        let TransferStrategy::Split {
            partial_coins,
            split_coin,
            target_denominations,
            change_denominations,
        } = result.strategy
        else {
            panic!("expected split");
        };
        assert!(partial_coins.is_empty());
        assert_eq!(split_coin.derivation_index, 2);
        // target 30 = 20 + 10; change 50 = 40 + 10.
        assert_eq!(
            target_denominations,
            [Denomination { exponent: 1 }, Denomination { exponent: 0 }]
        );
        assert_eq!(
            change_denominations,
            [Denomination { exponent: 2 }, Denomination { exponent: 0 }]
        );
        // Split invariant: partial + target == amount; target + change == coin.
        assert_eq!(ctx().total_value(&target_denominations), 30);
        assert_eq!(ctx().total_value(&change_denominations), 50);
    }

    /// Smaller whole coins must not be added when one coin can fund the full
    /// split.
    #[test]
    fn split_uses_one_smallest_coin_larger_than_the_whole_request() {
        let coins = [
            coin(1, 4, None), // 160
            coin(2, 2, None), // 40
            coin(3, 0, None), // 10
        ];
        let result = selector().select(30, &coins, &[], NOW).unwrap();
        let TransferStrategy::Split {
            partial_coins,
            split_coin,
            ..
        } = result.strategy
        else {
            panic!("expected split");
        };
        assert!(partial_coins.is_empty());
        assert_eq!(split_coin.derivation_index, 2);
    }

    #[test]
    fn split_takes_a_partial_contribution_first() {
        // Amount 50: greedy takes the 40 (exp 2), remainder 10 needs a
        // split of the 20 (exp 1) into 10 + 10.
        let coins = [coin(1, 2, None), coin(2, 1, None)];
        let result = selector().select(50, &coins, &[], NOW).unwrap();
        let TransferStrategy::Split {
            partial_coins,
            split_coin,
            target_denominations,
            change_denominations,
        } = result.strategy
        else {
            panic!("expected split");
        };
        assert_eq!(partial_coins[0].derivation_index, 1);
        assert_eq!(split_coin.derivation_index, 2);
        assert_eq!(ctx().total_value(&target_denominations), 10);
        assert_eq!(ctx().total_value(&change_denominations), 10);
    }

    #[test]
    fn unload_prefers_single_smallest_sufficient_voucher() {
        let coins: [Coin; 0] = [];
        let vouchers = [full_voucher(1, 4), full_voucher(2, 2), full_voucher(3, 3)];
        // Need 40: the exp-2 voucher (value 40) is the minimal cover —
        // not the 80, not the 160, not a combination.
        let result = selector().select(40, &coins, &vouchers, NOW).unwrap();
        let TransferStrategy::UnloadIntoCoins {
            coins: partial,
            groups,
        } = result.strategy
        else {
            panic!("expected unload");
        };
        assert!(partial.is_empty());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].vouchers[0].derivation_index, 2);
        assert_eq!(ctx().total_value(&groups[0].recipient_denominations), 40);
        assert!(groups[0].change_denominations.is_empty());
        assert_eq!(result.privacy_level, PrivacyLevel::Full);
    }

    #[test]
    fn unload_groups_by_recycler_and_balances_output_to_input() {
        let vouchers = [
            voucher(1, 2, 7, VoucherPrivacyLevel::Full, 0), // 40, recycler (2,7)
            voucher(2, 2, 9, VoucherPrivacyLevel::Full, 0), // 40, recycler (2,9)
        ];
        // Need 60 → both vouchers (no single is sufficient): group (2,7)
        // gives 40 to the recipient, group (2,9) gives 20 + 20 change.
        let result = selector().select(60, &[], &vouchers, NOW).unwrap();
        let TransferStrategy::UnloadIntoCoins { groups, .. } = result.strategy else {
            panic!("expected unload");
        };
        assert_eq!(groups.len(), 2);
        for group in &groups {
            let input: u128 = group
                .vouchers
                .iter()
                .map(|v| ctx().value_in_planks(v.exponent))
                .sum();
            let output = ctx().total_value(&group.recipient_denominations)
                + ctx().total_value(&group.change_denominations);
            assert_eq!(input, output, "pallet invariant: group output == input");
        }
        let recipient_total: u128 = groups
            .iter()
            .map(|g| ctx().total_value(&g.recipient_denominations))
            .sum();
        assert_eq!(recipient_total, 60);
    }

    #[test]
    fn unload_groups_are_ordered_by_descending_exponent() {
        let vouchers = [
            voucher(1, 0, 7, VoucherPrivacyLevel::Full, 0), // 10
            voucher(2, 2, 9, VoucherPrivacyLevel::Full, 0), // 40
        ];
        let result = selector().select(50, &[], &vouchers, NOW).unwrap();
        let TransferStrategy::UnloadIntoCoins { groups, .. } = result.strategy else {
            panic!("expected unload");
        };
        assert_eq!(
            groups
                .iter()
                .map(|group| group.recycler.exponent)
                .collect::<Vec<_>>(),
            [2, 0]
        );
    }

    #[test]
    fn unload_uses_partial_coins_before_vouchers() {
        let coins = [coin(1, 1, None)]; // 20
        let vouchers = [full_voucher(10, 2)]; // 40
        // Need 60 = coin 20 + voucher 40.
        let result = selector().select(60, &coins, &vouchers, NOW).unwrap();
        let TransferStrategy::UnloadIntoCoins {
            coins: partial,
            groups,
        } = result.strategy
        else {
            panic!("expected unload");
        };
        assert_eq!(partial[0].derivation_index, 1);
        assert_eq!(groups[0].vouchers[0].derivation_index, 10);
    }

    #[test]
    fn degraded_fallback_triggers_when_full_privacy_is_insufficient() {
        let vouchers = [
            full_voucher(1, 1),                                 // 20 full
            voucher(2, 2, 0, VoucherPrivacyLevel::Degraded, 0), // 40 degraded
        ];
        // 20 is coverable full-privacy → Full.
        let result = selector().select(20, &[], &vouchers, NOW).unwrap();
        assert_eq!(result.privacy_level, PrivacyLevel::Full);
        // 60 needs the degraded voucher too → Degraded.
        let result = selector().select(60, &[], &vouchers, NOW).unwrap();
        assert_eq!(result.privacy_level, PrivacyLevel::Degraded);
    }

    #[test]
    fn not_yet_ready_full_voucher_counts_as_degraded() {
        let vouchers = [voucher(1, 1, 0, VoucherPrivacyLevel::Full, NOW + 1)];
        let result = selector().select(20, &[], &vouchers, NOW).unwrap();
        assert_eq!(result.privacy_level, PrivacyLevel::Degraded);
    }

    #[test]
    fn no_ready_vouchers_error_when_pool_is_all_unready() {
        let vouchers = [voucher(1, 0, 0, VoucherPrivacyLevel::Degraded, 0)]; // 10
        assert_eq!(
            selector().select(160, &[], &vouchers, NOW),
            Err(CoinSelectionError::NoReadyVouchers)
        );
    }

    #[test]
    fn too_many_vouchers_in_group_is_a_hard_stop() {
        let tight = CoinSelector::new(ctx(), 2);
        let vouchers: Vec<Voucher> = (0..3).map(|i| full_voucher(i, 0)).collect(); // 3 × 10, one recycler
        assert_eq!(
            tight.select(30, &[], &vouchers, NOW),
            Err(CoinSelectionError::TooManyVouchersInGroup { count: 3, max: 2 })
        );
    }

    #[test]
    fn voucher_group_equal_to_max_is_rejected_like_ios_v2() {
        let tight = CoinSelector::new(ctx(), 2);
        let vouchers = [full_voucher(1, 0), full_voucher(2, 0)];
        assert_eq!(
            tight.select(20, &[], &vouchers, NOW),
            Err(CoinSelectionError::TooManyVouchersInGroup { count: 2, max: 2 })
        );
    }

    #[test]
    fn insufficient_funds_when_everything_together_cannot_cover() {
        let coins = [coin(1, 0, None)];
        let vouchers = [full_voucher(2, 0)];
        assert_eq!(
            selector().select(160, &coins, &vouchers, NOW),
            Err(CoinSelectionError::InsufficientFunds)
        );
    }

    /// Deterministic pseudo-random sweep: every outcome upholds the value
    /// invariants, and all three strategy paths are exercised.
    #[test]
    fn random_sweep_covers_all_strategies_and_holds_invariants() {
        use rand::rngs::StdRng;
        use rand::{Rng, SeedableRng};
        let mut rng = StdRng::seed_from_u64(0x_C01_A6E);
        let context = ctx();
        let selector = selector();
        let (mut exact, mut split, mut unload) = (0, 0, 0);

        for _ in 0..300 {
            let coins: Vec<Coin> = (0..rng.gen_range(0..6))
                .map(|i| {
                    coin(
                        i,
                        rng.gen_range(0..=4),
                        if rng.gen_bool(0.3) {
                            None
                        } else {
                            Some(rng.gen_range(0..16))
                        },
                    )
                })
                .collect();
            let vouchers: Vec<Voucher> = (0..rng.gen_range(0..5))
                .map(|i| {
                    voucher(
                        100 + i,
                        rng.gen_range(0..=4),
                        rng.gen_range(0..3),
                        if rng.gen_bool(0.7) {
                            VoucherPrivacyLevel::Full
                        } else {
                            VoucherPrivacyLevel::Degraded
                        },
                        if rng.gen_bool(0.8) { 0 } else { NOW + 1 },
                    )
                })
                .collect();
            let amount = u128::from(rng.gen_range(1..60u32)) * 10;

            let first = selector.select(amount, &coins, &vouchers, NOW);
            assert_eq!(
                first,
                selector.select(amount, &coins, &vouchers, NOW),
                "identical snapshots must select deterministically"
            );

            match first {
                Ok(result) => match result.strategy {
                    TransferStrategy::ExactMatch { coins: selected } => {
                        let unique: std::collections::HashSet<_> =
                            selected.iter().map(|coin| coin.derivation_index).collect();
                        assert_eq!(unique.len(), selected.len(), "coin inputs are unique");
                        exact += 1;
                        let total: u128 = selected
                            .iter()
                            .map(|c| context.value_in_planks(c.exponent))
                            .sum();
                        assert_eq!(total, amount);
                        assert!(selected.iter().all(|c| c.is_selectable()));
                    }
                    TransferStrategy::Split {
                        partial_coins,
                        split_coin,
                        target_denominations,
                        change_denominations,
                    } => {
                        split += 1;
                        let mut unique: std::collections::HashSet<_> = partial_coins
                            .iter()
                            .map(|coin| coin.derivation_index)
                            .collect();
                        assert!(
                            unique.insert(split_coin.derivation_index),
                            "the split coin cannot also be a partial input"
                        );
                        assert!(partial_coins.iter().all(Coin::is_selectable));
                        assert!(split_coin.is_selectable());
                        let partial: u128 = partial_coins
                            .iter()
                            .map(|c| context.value_in_planks(c.exponent))
                            .sum();
                        assert_eq!(
                            partial + context.total_value(&target_denominations),
                            amount,
                            "partial + split target == amount"
                        );
                        assert_eq!(
                            context.total_value(&target_denominations)
                                + context.total_value(&change_denominations),
                            context.value_in_planks(split_coin.exponent),
                            "split conserves the coin's value"
                        );
                    }
                    TransferStrategy::UnloadIntoCoins {
                        coins: partial,
                        groups,
                    } => {
                        unload += 1;
                        let unique_coins: std::collections::HashSet<_> =
                            partial.iter().map(|coin| coin.derivation_index).collect();
                        assert_eq!(unique_coins.len(), partial.len(), "coin inputs are unique");
                        assert!(partial.iter().all(Coin::is_selectable));
                        let mut unique_vouchers = std::collections::HashSet::new();
                        let partial: u128 = partial
                            .iter()
                            .map(|c| context.value_in_planks(c.exponent))
                            .sum();
                        let recipient: u128 = groups
                            .iter()
                            .map(|g| context.total_value(&g.recipient_denominations))
                            .sum();
                        assert_eq!(partial + recipient, amount, "unload covers exactly");
                        for group in &groups {
                            let input: u128 = group
                                .vouchers
                                .iter()
                                .map(|v| context.value_in_planks(v.exponent))
                                .sum();
                            assert_eq!(
                                input,
                                context.total_value(&group.recipient_denominations)
                                    + context.total_value(&group.change_denominations)
                            );
                            assert!(
                                group.vouchers.len() < 8,
                                "reference max-consolidation check is strict"
                            );
                            assert!(group.vouchers.iter().all(Voucher::is_unloadable));
                            assert!(
                                group.vouchers.iter().all(|voucher| {
                                    unique_vouchers.insert(voucher.derivation_index)
                                }),
                                "voucher inputs are unique across recycler groups"
                            );
                        }
                    }
                },
                Err(error) => {
                    assert!(
                        matches!(
                            error,
                            CoinSelectionError::EmptyWallet
                                | CoinSelectionError::InsufficientFunds
                                | CoinSelectionError::NoReadyVouchers
                                | CoinSelectionError::TooManyVouchersInGroup { .. }
                        ),
                        "unexpected error class: {error:?}"
                    );
                }
            }
        }
        assert!(exact > 10, "exact-match path exercised ({exact})");
        assert!(split > 10, "split path exercised ({split})");
        assert!(unload > 10, "unload path exercised ({unload})");
    }
}
