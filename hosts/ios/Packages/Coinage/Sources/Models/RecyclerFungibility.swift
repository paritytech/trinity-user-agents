import Foundation

/// Derives the recycler-fungibility scores persisted on vouchers and coins from live recycler state.
///
/// Both scores express, on the `0...100` scale, how much of a recycler's anonymity is still intact.
/// The inputs are `L` — the ring capacity ("max recycler positions"), which is per denomination and
/// not a runtime-wide constant — `I`, the ring's included member count, and `U`, the ring's
/// `RecyclersUnloadedCount`.
///
/// `U` is sanitised once, against `I`, before either score is computed: the chain decrements the
/// unloaded count when a dispatch fails, so a reading can transiently exceed the members it refers
/// to. Clamping to `L` instead would let such a reading depress the ceiling — which is frozen, so a
/// momentary bad value would stick permanently.
public enum RecyclerFungibility {
    /// The best score a voucher entering this ring could have had: `100 · (L² − U²) / L²`, which is
    /// ``current(included:unloaded:capacity:)`` evaluated at a full ring.
    ///
    /// Frozen onto the voucher the first time it is observed in a ring — the ring index is assigned
    /// on chain, so it is not knowable when the voucher is minted. `I` is taken only to sanitise `U`
    /// by the same rule the current score uses.
    public static func maximum(included: UInt32, unloaded: UInt32, capacity: Int) -> UInt8 {
        let includedCount = Int(included)

        guard capacity > 0, includedCount > 0 else { return 0 }

        let clampedUnloaded = min(Int(unloaded), includedCount)

        return percentage(
            numerator: capacity * capacity - clampedUnloaded * clampedUnloaded,
            denominator: capacity * capacity
        )
    }

    /// The ring's fungibility as it stands now: `100 · (I² − U²) / (I · L)`.
    ///
    /// An empty ring scores 0 rather than dividing by zero — a ring with no included members offers
    /// no anonymity, and the state is reachable transiently when a fork retracts a ring.
    public static func current(included: UInt32, unloaded: UInt32, capacity: Int) -> UInt8 {
        let includedCount = Int(included)

        guard capacity > 0, includedCount > 0 else { return 0 }

        let clampedUnloaded = min(Int(unloaded), includedCount)

        return percentage(
            numerator: includedCount * includedCount - clampedUnloaded * clampedUnloaded,
            denominator: includedCount * capacity
        )
    }
}

private extension RecyclerFungibility {
    /// Exact integer percentage, rounded half up. Stays clamped to the scale even if the chain
    /// reports more included members than the collection's capacity.
    static func percentage(numerator: Int, denominator: Int) -> UInt8 {
        guard denominator > 0, numerator > 0 else { return 0 }

        let scaled = 100 * numerator
        let rounded = (2 * scaled + denominator) / (2 * denominator)

        return UInt8(min(rounded, Int(CoinageConstants.fullFungibility)))
    }
}
