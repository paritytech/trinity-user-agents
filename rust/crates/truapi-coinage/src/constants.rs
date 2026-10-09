// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use std::time::Duration;

pub const CASH_ASSET_PRECISION: u8 = 6;

/// The shipped runtime's `Coinage.UnderlyingAssetUnit`: raw CASH planks per
/// user-facing cent, `10^(precision - 2)`. Live chain metadata remains the
/// authority wherever it is read — this is the pre-live default for display
/// projections (a fresh recipient renders a received transfer in chat before
/// any wallet flow has fetched the live constants; a placeholder of `1`
/// there showed a 2-CASH transfer as "20,000").
pub const CASH_PLANKS_PER_CENT: u128 = 10u128.pow((CASH_ASSET_PRECISION - 2) as u32);

pub const COIN_MAX_AGE: i16 = 16;

/// Coins with `age >= RECYCLE_AT_AGE` are excluded from selection so they can
/// be recycled before reaching `COIN_MAX_AGE`. Equals `COIN_MAX_AGE - 2`.
pub const RECYCLE_AT_AGE: i16 = 14;

pub const MINIMUM_RING_SIZE: u32 = 10;

pub const WAL_MORTALITY_BLOCKS: u64 = 300;

/// Longest a voucher may sit waiting before onboarding is abandoned
/// (`maxVoucherWaitTime`, 6 hours).
pub const MAX_VOUCHER_WAIT_TIME: Duration = Duration::from_secs(6 * 60 * 60);

pub const SEND_VERIFY_BLOCK_TIMEOUT: u32 = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_constants_are_pinned() {
        assert_eq!(COIN_MAX_AGE, 16);
        assert_eq!(RECYCLE_AT_AGE, 14);
        assert_eq!(COIN_MAX_AGE - 2, RECYCLE_AT_AGE);
        assert_eq!(MINIMUM_RING_SIZE, 10);
        assert_eq!(WAL_MORTALITY_BLOCKS, 300);
        assert_eq!(MAX_VOUCHER_WAIT_TIME, Duration::from_secs(21_600));
        assert_eq!(SEND_VERIFY_BLOCK_TIMEOUT, 100);
        assert_eq!(CASH_ASSET_PRECISION, 6);
        assert_eq!(CASH_PLANKS_PER_CENT, 10_000);
    }
}
