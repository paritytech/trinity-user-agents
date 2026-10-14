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
    /// Ready at the required ring fill, or with enough members and enough time since confirmed inclusion.
    case ringFillOrMembersAndAge(
        requiredRingFill: BigRational,
        minimumMembers: UInt32,
        minimumAge: TimeInterval
    )

    func readyAt(for voucher: Voucher) -> Date? {
        guard case let .ringFillOrMembersAndAge(_, minimumMembers, minimumAge) = self,
              let recycler = voucher.recycler,
              recycler.membersCount >= minimumMembers,
              let enteredAt = recycler.enteredAt else { return nil }

        return enteredAt.addingTimeInterval(minimumAge)
    }
}
