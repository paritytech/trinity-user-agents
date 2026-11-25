import Foundation

/// Candidates arrive ordered by priority and age, so the first cooled-down one wins.
enum LegacyEvictionPolicy {
    static func firstCooledDown(
        _ candidates: [SSSEvictionCandidate],
        cooldown: UInt32,
        nowSeconds: UInt64
    ) -> SSSEvictionCandidate? {
        candidates.first { nowSeconds >= $0.since + UInt64(cooldown) }
    }
}
