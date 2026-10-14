import Foundation

/// Current time and ring capacities used to evaluate voucher readiness.
public struct VoucherUsabilityContext: Equatable {
    /// Capacity (max provable ring members) keyed by voucher exponent.
    public let ringCapacities: [Int16: Int]
    public let now: Date

    public init(ringCapacities: [Int16: Int], now: Date) {
        self.ringCapacities = ringCapacities
        self.now = now
    }

    public func capacity(for exponent: Int16) -> Int? {
        ringCapacities[exponent]
    }
}
