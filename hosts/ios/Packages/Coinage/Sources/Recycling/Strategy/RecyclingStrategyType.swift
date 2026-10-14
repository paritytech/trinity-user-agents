import Foundation
import SubstrateSdk

/// The user-chosen privacy preset. The raw value is the persisted key.
public enum RecyclingStrategyType: String, CaseIterable, Equatable, Sendable {
    case minPrivacy
    case balanced
    case maxPrivacy
}

public extension RecyclingStrategyType {
    /// Unload mints a coin's successor at age 1, so nothing below this can exist on chain.
    static let minRecyclableAge: Int16 = 1

    private static let balancedAgeDivisor: Int16 = 3
    private static let minimumVoucherMembers: UInt32 = 32
    private static let minimumVoucherAge: TimeInterval = 10 * 60

    /// Resolves the preset into concrete parameters. `forcedRecyclingAge` is the chain ceiling
    /// (`getCoinRecyclingAge()` = `coinMaxAge - 2`), the anchor the presets are expressed against.
    func params(forcedRecyclingAge: Int16) -> RecyclingParams {
        switch self {
        case .minPrivacy:
            // Zero budget never voluntarily gates; only the chain-limits decorator forces coins at
            // the ceiling. Reproduces today's behaviour, which is why it is the default.
            RecyclingParams(
                maxUnavailableBalance: .percent(of: 0),
                minRecyclingAge: forcedRecyclingAge,
                voucherReadiness: .immediate,
                allowsConfirmedSpend: true
            )
        case .balanced:
            RecyclingParams(
                maxUnavailableBalance: .percent(of: 20),
                minRecyclingAge: max(Self.minRecyclableAge, forcedRecyclingAge / Self.balancedAgeDivisor),
                voucherReadiness: .ringFillOrMembersAndAge(
                    requiredRingFill: .percent(of: 20),
                    minimumMembers: Self.minimumVoucherMembers,
                    minimumAge: Self.minimumVoucherAge
                ),
                allowsConfirmedSpend: true
            )
        case .maxPrivacy:
            // Gates from age 1: unload mints an age-1 successor that is immediately eligible again.
            // Intended — max privacy leans on the quota valve continuously.
            RecyclingParams(
                maxUnavailableBalance: .percent(of: 100),
                minRecyclingAge: Self.minRecyclableAge,
                voucherReadiness: .ringFillOrMembersAndAge(
                    requiredRingFill: .percent(of: 90),
                    minimumMembers: Self.minimumVoucherMembers,
                    minimumAge: Self.minimumVoucherAge
                ),
                allowsConfirmedSpend: false
            )
        }
    }
}
