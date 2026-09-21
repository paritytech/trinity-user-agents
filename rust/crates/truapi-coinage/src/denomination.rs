// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

/// One power-of-two denomination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Denomination {
    pub exponent: i16,
}

/// Greedy decomposition outcome: the denominations that fit, plus the
/// remainder below the smallest denomination (zero when the amount is
/// exactly representable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenominationBreakdown {
    pub denominations: Vec<Denomination>,
    pub remainder: u128,
}

impl DenominationBreakdown {
    pub fn is_exact(&self) -> bool {
        self.remainder == 0
    }
}

/// The pallet-constant context driving all denomination math.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenominationBreakdownContext {
    /// `UnderlyingAssetUnit` pallet constant, in planks.
    pub asset_unit: u128,
    /// `MaximumExponent` pallet constant.
    pub max_exponent: i16,
    /// `MinimumExponent` pallet constant (may be negative).
    pub min_exponent: i16,
    pub precision: u8,
}

impl DenominationBreakdownContext {
    /// Converts the UI/protocol CASH balance unit (one cent) into the raw
    /// planks consumed by Coinage selection and emitted in transfer memos.
    /// `UnderlyingAssetUnit` is exactly that cent in the active asset.
    pub fn cash_cents_to_planks(&self, cents: u128) -> Option<u128> {
        cents.checked_mul(self.asset_unit)
    }

    /// Converts raw Coinage planks back into whole CASH cents. A remainder
    /// is rejected rather than rounded across a payment confirmation edge.
    pub fn cash_cents_from_planks(&self, planks: u128) -> Option<u128> {
        (self.asset_unit != 0 && planks.is_multiple_of(self.asset_unit))
            .then(|| planks / self.asset_unit)
    }

    /// Denomination value in planks: `unit << exponent` for non-negative
    /// exponents, `unit >> -exponent` for negative ones. Each direction
    /// saturates (`u128::MAX` or `0`) rather than panicking on an absurd
    /// shift amount.
    pub fn value_in_planks(&self, exponent: i16) -> u128 {
        if exponent >= 0 {
            self.asset_unit
                .checked_shl(u32::from(exponent as u16))
                .unwrap_or(u128::MAX)
        } else {
            let shift = u32::from(exponent.unsigned_abs());
            if shift >= 128 {
                0
            } else {
                self.asset_unit >> shift
            }
        }
    }

    /// Greedy binary decomposition from `max_exponent` down to `min_exponent`,
    /// taking as many of each denomination as fit; anything below the
    /// smallest denomination is returned as `remainder`.
    pub fn breakdown(&self, amount_planks: u128) -> DenominationBreakdown {
        let mut remaining = amount_planks;
        let mut denominations = Vec::new();
        let mut exponent = self.max_exponent;
        while exponent >= self.min_exponent {
            let value = self.value_in_planks(exponent);
            if value > 0 {
                while remaining >= value {
                    denominations.push(Denomination { exponent });
                    remaining -= value;
                }
            }
            exponent -= 1;
        }
        DenominationBreakdown {
            denominations,
            remainder: remaining,
        }
    }

    /// Total planks of a denomination list.
    pub fn total_value(&self, denominations: &[Denomination]) -> u128 {
        denominations.iter().fold(0u128, |acc, d| {
            acc.saturating_add(self.value_in_planks(d.exponent))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit 10, exponents 0..=4 → denominations 10, 20, 40, 80, 160.
    fn ctx() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 4,
            min_exponent: 0,
            precision: 10,
        }
    }

    #[test]
    fn value_in_planks_shifts_both_directions() {
        let ctx = ctx();
        assert_eq!(ctx.value_in_planks(0), 10);
        assert_eq!(ctx.value_in_planks(3), 80);
        let fractional = DenominationBreakdownContext {
            asset_unit: 16,
            max_exponent: 2,
            min_exponent: -2,
            precision: 0,
        };
        assert_eq!(fractional.value_in_planks(-1), 8);
        assert_eq!(fractional.value_in_planks(-2), 4);
    }

    #[test]
    fn cash_cents_and_asset_planks_have_an_explicit_exact_boundary() {
        let context = DenominationBreakdownContext {
            asset_unit: 10_000,
            max_exponent: 16,
            min_exponent: 0,
            precision: 6,
        };

        for (cash, cents, planks) in [
            (1, 100, 1_000_000),
            (10, 1_000, 10_000_000),
            (100, 10_000, 100_000_000),
            (200, 20_000, 200_000_000),
        ] {
            assert_eq!(
                context.cash_cents_to_planks(cents),
                Some(planks),
                "{cash} CASH"
            );
            assert_eq!(
                context.cash_cents_from_planks(planks),
                Some(cents),
                "{cash} CASH"
            );
        }
        assert_eq!(context.cash_cents_from_planks(9_999), None);
        assert_eq!(context.cash_cents_to_planks(u128::MAX), None);
    }

    #[test]
    fn breakdown_of_known_amounts_is_greedy_largest_first() {
        let ctx = ctx();
        // 230 = 160 + 40 + 20 + 10.
        let b = ctx.breakdown(230);
        assert_eq!(
            b.denominations,
            [
                Denomination { exponent: 4 },
                Denomination { exponent: 2 },
                Denomination { exponent: 1 },
                Denomination { exponent: 0 },
            ]
        );
        assert!(b.is_exact());
        // 320 = 160 + 160 (repeated denominations allowed).
        let b = ctx.breakdown(320);
        assert_eq!(
            b.denominations,
            [Denomination { exponent: 4 }, Denomination { exponent: 4 }]
        );
        assert!(b.is_exact());
    }

    #[test]
    fn breakdown_reports_sub_denomination_remainder() {
        let ctx = ctx();
        let b = ctx.breakdown(235);
        assert_eq!(b.remainder, 5, "5 planks sit below the 10-plank unit");
        assert!(!b.is_exact());
        assert_eq!(ctx.total_value(&b.denominations), 230);
    }

    #[test]
    fn breakdown_of_zero_is_empty_and_exact() {
        let b = ctx().breakdown(0);
        assert!(b.denominations.is_empty());
        assert!(b.is_exact());
    }

    #[test]
    fn negative_exponents_extend_below_the_unit() {
        let ctx = DenominationBreakdownContext {
            asset_unit: 16,
            max_exponent: 1,
            min_exponent: -2,
            precision: 0,
        };
        // 28 = 16 + 8 + 4.
        let b = ctx.breakdown(28);
        assert_eq!(
            b.denominations,
            [
                Denomination { exponent: 0 },
                Denomination { exponent: -1 },
                Denomination { exponent: -2 },
            ]
        );
        assert!(b.is_exact());
    }

    /// Breakdown reconstructs: `total_value(breakdown(x)) + remainder == x`
    /// across a sweep — the invariant coin allocation relies on.
    #[test]
    fn breakdown_reconstructs_every_amount() {
        let ctx = ctx();
        for amount in 0..2_000u128 {
            let b = ctx.breakdown(amount);
            assert_eq!(ctx.total_value(&b.denominations) + b.remainder, amount);
            assert!(b.remainder < ctx.value_in_planks(ctx.min_exponent));
        }
    }
}
