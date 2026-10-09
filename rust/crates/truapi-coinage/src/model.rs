// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use crate::constants::{COIN_MAX_AGE, MINIMUM_RING_SIZE, RECYCLE_AT_AGE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinState {
    Available,
    /// A recycle-into-voucher extrinsic is in flight.
    Recycling,
    /// Reserved for an outgoing transfer (set before submission,
    /// reverted on failure).
    PendingTransfer,
    Spent,
}

impl CoinState {
    pub fn can_transition_to(self, to: CoinState) -> bool {
        use CoinState::*;
        matches!(
            (self, to),
            (Available, Recycling)
                | (Available, PendingTransfer)
                | (Recycling, Spent)
                | (Recycling, Available)
                | (PendingTransfer, Spent)
                | (PendingTransfer, Available)
                | (Spent, Available)
        )
    }

    /// Stable integer for the `coins.state` column.
    pub fn as_raw(self) -> i64 {
        match self {
            CoinState::Available => 0,
            CoinState::Recycling => 1,
            CoinState::PendingTransfer => 2,
            CoinState::Spent => 3,
        }
    }

    pub fn from_raw(raw: i64) -> Option<Self> {
        Some(match raw {
            0 => CoinState::Available,
            1 => CoinState::Recycling,
            2 => CoinState::PendingTransfer,
            3 => CoinState::Spent,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coin {
    pub exponent: i16,
    pub derivation_index: u32,
    pub age: Option<i16>,
    pub state: CoinState,
}

impl Coin {
    pub fn is_expiring_soon(&self) -> bool {
        matches!(self.age, Some(age) if age >= RECYCLE_AT_AGE)
    }

    pub fn is_chain_invalid(&self) -> bool {
        matches!(self.age, Some(age) if age >= COIN_MAX_AGE)
    }

    /// Eligible for transfer selection: available and not expiring soon.
    pub fn is_selectable(&self) -> bool {
        self.state == CoinState::Available && !self.is_expiring_soon()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoucherRemoteState {
    /// Not yet observed anywhere on-chain (also the state of freshly
    /// recovered vouchers before `VoucherLocationService` reconciles).
    Unlocated,
    /// Ring membership submitted, not yet included.
    Onboarding,
    /// Included in a recycler ring at this index.
    InRecycler { recycler_index: u32 },
    /// Consumed by an unload extrinsic.
    Unloaded,
}

impl VoucherRemoteState {
    pub fn is_in_recycler(&self) -> bool {
        matches!(self, VoucherRemoteState::InRecycler { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoucherLocalState {
    Available,
    PendingTransfer,
    PendingOnboarding,
    Spent,
}

impl VoucherLocalState {
    pub fn as_raw(self) -> i64 {
        match self {
            VoucherLocalState::Available => 0,
            VoucherLocalState::PendingTransfer => 1,
            VoucherLocalState::PendingOnboarding => 2,
            VoucherLocalState::Spent => 3,
        }
    }

    pub fn from_raw(raw: i64) -> Option<Self> {
        Some(match raw {
            0 => VoucherLocalState::Available,
            1 => VoucherLocalState::PendingTransfer,
            2 => VoucherLocalState::PendingOnboarding,
            3 => VoucherLocalState::Spent,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoucherPrivacyLevel {
    Degraded,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectivePrivacy {
    Degraded,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voucher {
    pub exponent: i16,
    pub derivation_index: u32,
    pub allocated_at_ms: i64,
    pub ready_at_ms: i64,
    pub remote_state: VoucherRemoteState,
    pub local_state: VoucherLocalState,
    pub privacy: VoucherPrivacyLevel,
}

impl Voucher {
    pub fn effective_privacy(&self, now_ms: i64) -> EffectivePrivacy {
        if self.privacy == VoucherPrivacyLevel::Full && now_ms >= self.ready_at_ms {
            EffectivePrivacy::Full
        } else {
            EffectivePrivacy::Degraded
        }
    }

    /// Spendable through an unload strategy: locally available and
    /// on-chain in a recycler.
    pub fn is_unloadable(&self) -> bool {
        self.local_state == VoucherLocalState::Available && self.remote_state.is_in_recycler()
    }
}

pub fn ring_readiness_upgraded(included_members: u32) -> bool {
    included_members >= MINIMUM_RING_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coin(age: Option<i16>, state: CoinState) -> Coin {
        Coin {
            exponent: 0,
            derivation_index: 1,
            age,
            state,
        }
    }

    #[test]
    fn coin_state_machine_progression() {
        use CoinState::*;
        // The forward paths.
        assert!(Available.can_transition_to(Recycling));
        assert!(Available.can_transition_to(PendingTransfer));
        assert!(Recycling.can_transition_to(Spent));
        assert!(PendingTransfer.can_transition_to(Spent));
        assert!(Recycling.can_transition_to(Available));
        assert!(PendingTransfer.can_transition_to(Available));
        assert!(Spent.can_transition_to(Available));
        // Undocumented jumps are rejected.
        assert!(!Available.can_transition_to(Spent));
        assert!(!Available.can_transition_to(Available));
        assert!(!Spent.can_transition_to(Recycling));
        assert!(!Spent.can_transition_to(PendingTransfer));
        assert!(!Recycling.can_transition_to(PendingTransfer));
        assert!(!PendingTransfer.can_transition_to(Recycling));
    }

    #[test]
    fn coin_state_raw_round_trips() {
        for state in [
            CoinState::Available,
            CoinState::Recycling,
            CoinState::PendingTransfer,
            CoinState::Spent,
        ] {
            assert_eq!(CoinState::from_raw(state.as_raw()), Some(state));
        }
        assert_eq!(CoinState::from_raw(9), None);
    }

    #[test]
    fn expiring_soon_at_the_recycle_age_boundary() {
        assert!(!coin(Some(13), CoinState::Available).is_expiring_soon());
        assert!(coin(Some(14), CoinState::Available).is_expiring_soon());
        assert!(coin(Some(16), CoinState::Available).is_expiring_soon());
        assert!(!coin(None, CoinState::Available).is_expiring_soon());
    }

    #[test]
    fn chain_invalid_at_max_age() {
        assert!(!coin(Some(15), CoinState::Available).is_chain_invalid());
        assert!(coin(Some(16), CoinState::Available).is_chain_invalid());
    }

    #[test]
    fn selectable_excludes_expiring_and_non_available() {
        assert!(coin(Some(13), CoinState::Available).is_selectable());
        assert!(!coin(Some(14), CoinState::Available).is_selectable());
        assert!(!coin(Some(1), CoinState::Recycling).is_selectable());
        assert!(!coin(Some(1), CoinState::PendingTransfer).is_selectable());
        assert!(!coin(Some(1), CoinState::Spent).is_selectable());
    }

    fn voucher(privacy: VoucherPrivacyLevel, ready_at_ms: i64) -> Voucher {
        Voucher {
            exponent: 0,
            derivation_index: 1,
            allocated_at_ms: 0,
            ready_at_ms,
            remote_state: VoucherRemoteState::InRecycler { recycler_index: 0 },
            local_state: VoucherLocalState::Available,
            privacy,
        }
    }

    #[test]
    fn full_privacy_voucher_is_degraded_before_ready_at() {
        let v = voucher(VoucherPrivacyLevel::Full, 1_000);
        assert_eq!(v.effective_privacy(999), EffectivePrivacy::Degraded);
        assert_eq!(v.effective_privacy(1_000), EffectivePrivacy::Full);
        assert_eq!(v.effective_privacy(2_000), EffectivePrivacy::Full);
    }

    #[test]
    fn degraded_voucher_never_upgrades_by_time() {
        let v = voucher(VoucherPrivacyLevel::Degraded, 1_000);
        assert_eq!(v.effective_privacy(i64::MAX), EffectivePrivacy::Degraded);
    }

    #[test]
    fn ring_readiness_upgrades_exactly_at_the_minimum() {
        assert!(!ring_readiness_upgraded(MINIMUM_RING_SIZE - 1));
        assert!(ring_readiness_upgraded(MINIMUM_RING_SIZE));
        assert!(ring_readiness_upgraded(MINIMUM_RING_SIZE + 1));
    }

    #[test]
    fn minimum_ring_size_is_ten() {
        assert_eq!(MINIMUM_RING_SIZE, 10);
    }

    #[test]
    fn unloadable_requires_local_available_and_in_recycler() {
        let mut v = voucher(VoucherPrivacyLevel::Full, 0);
        assert!(v.is_unloadable());
        v.remote_state = VoucherRemoteState::Onboarding;
        assert!(!v.is_unloadable());
        v.remote_state = VoucherRemoteState::InRecycler { recycler_index: 2 };
        v.local_state = VoucherLocalState::PendingTransfer;
        assert!(!v.is_unloadable());
    }
}
