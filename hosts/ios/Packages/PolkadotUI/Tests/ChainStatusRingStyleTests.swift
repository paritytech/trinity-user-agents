import Testing
@testable import PolkadotUI

/// Targets `arcBand` rather than `arcColor`: a `Color` backed by a dynamic asset is a fresh
/// instance on every access, so comparing two of them always fails and comparing any two
/// always "differs" — an equality test on colours would be vacuous either way.
struct ChainStatusRingStyleTests {
    @Test(
        "Liveness falls in the band its value belongs to",
        arguments: [
            (0.0, ChainStatusRingStyle.ArcBand.critical),
            (0.1, .critical),
            (0.24, .critical),
            (0.25, .degraded),
            (0.4, .degraded),
            (0.49, .degraded),
            (0.5, .fair),
            (0.74, .fair),
            (0.75, .nearNormal),
            (0.8, .nearNormal)
        ]
    )
    func livenessFallsInItsBand(liveness: Double, expected: ChainStatusRingStyle.ArcBand) {
        #expect(ChainStatusRingStyle.arcBand(forLiveness: liveness) == expected)
    }

    @Test("Each boundary belongs to the band above it, not below")
    func boundariesRoundUpwards() {
        // The rule is "red under 25%", so 25% itself must not be red.
        #expect(ChainStatusRingStyle.arcBand(forLiveness: 0.25) != .critical)
        #expect(ChainStatusRingStyle.arcBand(forLiveness: 0.5) != .degraded)
        #expect(ChainStatusRingStyle.arcBand(forLiveness: 0.75) != .fair)
    }

    @Test("The largest reachable outage arc is no longer red")
    func largestOutageArcIsMonochrome() {
        // 8/10 at Nmax 10 and 12/15 at Nmax 15 are the biggest arcs `resolve` still calls an
        // outage. Both were red before the bands existed.
        #expect(ChainStatusRingStyle.arcBand(forLiveness: 0.8) == .nearNormal)
        #expect(ChainStatusRingStyle.arcBand(forLiveness: 12.0 / 15.0) == .nearNormal)
    }

    @Test("A chain producing nothing is critical")
    func noBlocksIsCritical() {
        #expect(ChainStatusRingStyle.arcBand(forLiveness: 0) == .critical)
    }

    @Test("Only a normal chain fills the disc")
    func onlyNormalFills() {
        #expect(ChainStatusRingStyle.isFilled(for: .normal))
        #expect(!ChainStatusRingStyle.isFilled(for: .outage(liveness: 0.8)))
        #expect(!ChainStatusRingStyle.isFilled(for: .dead))
    }
}
