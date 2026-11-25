import Foundation
import SubstrateSdk
import BigInt

/// Preset parameters for coin recycling and voucher readiness.
public struct RecyclingParams: Equatable {
    /// Share of total balance acceptable to hold *unavailable* while recycling. A ceiling, not a
    /// target — it exists so more than one coin can recycle at once.
    public let maxUnavailableBalance: BigRational

    /// Age below which recycling is not considered at all.
    public let minRecyclingAge: Int16

    /// Readiness rules for vouchers already included in a recycler ring.
    public let voucherReadiness: VoucherReadiness

    /// Whether gaining-privacy balance may be spent behind a confirmation.
    public let allowsConfirmedSpend: Bool

    public init(
        maxUnavailableBalance: BigRational,
        minRecyclingAge: Int16,
        voucherReadiness: VoucherReadiness,
        allowsConfirmedSpend: Bool
    ) {
        self.maxUnavailableBalance = maxUnavailableBalance
        self.minRecyclingAge = minRecyclingAge
        self.voucherReadiness = voucherReadiness
        self.allowsConfirmedSpend = allowsConfirmedSpend
    }
}

/// Readiness requirements for vouchers included in a recycler ring.
public enum VoucherReadiness: Equatable {
    /// Ready once inclusion in a recycler ring is confirmed.
    case immediate
    /// Ready at the required ring fill, or once minutes since confirmed inclusion × unclaimed ring members
    /// reaches `anonymityMinutes` × `anonymityMembers`.
    case ringFillOrAnonymityMinutes(
        requiredRingFill: BigRational,
        anonymityMinutes: UInt32,
        anonymityMembers: UInt32
    )

    private static let secondsPerMinute: TimeInterval = 60

    func readyAt(for voucher: Voucher) -> Date? {
        guard case let .ringFillOrAnonymityMinutes(_, anonymityMinutes, anonymityMembers) = self,
              let recycler = voucher.recycler,
              let enteredAt = recycler.enteredAt,
              let unloadedCount = recycler.unloadedCount else { return nil }

        let unclaimedMembers = recycler.membersCount - min(unloadedCount, recycler.membersCount)
        guard unclaimedMembers > 0 else { return nil }

        let delay = TimeInterval(anonymityMinutes) * Self.secondsPerMinute * TimeInterval(anonymityMembers)
            / TimeInterval(unclaimedMembers)
        return enteredAt.addingTimeInterval(delay)
    }
}
