import Foundation
import Testing
@testable import polkadot_app

/// The Collectibles page waits for the display to settle, so it does not expand while frames are still being dropped.
struct SteadyFrameRuleTests {
    /// The baseline is the shortest of the first three intervals, so a 0.4 s interval is slow against 0.25 s.
    @Test
    func baselineIsTheShortestOfTheFirstThreeIntervals() {
        var rule = SteadyFrameRule()

        let sampled = [rule.sample(0.5), rule.sample(0.25), rule.sample(0.75)]
        let slow = [rule.isSteady(after: 0.4), rule.isSteady(after: 0.4), rule.isSteady(after: 0.4)]

        #expect(sampled == [false, false, true])
        #expect(slow == [false, false, false])
    }

    /// Exactly 1.5 times the baseline still counts as steady.
    @Test
    func threeIntervalsWithinSlackAreSteady() {
        var rule = ruleSampled(atBaseline: 0.5)

        let steady = [rule.isSteady(after: 0.5), rule.isSteady(after: 0.6), rule.isSteady(after: 0.75)]

        #expect(steady == [false, false, true])
    }

    /// A frame longer than the slack starts the count again.
    @Test
    func aLongFrameRestartsTheRun() {
        var rule = ruleSampled(atBaseline: 0.5)

        let run = [
            rule.isSteady(after: 0.5),
            rule.isSteady(after: 1),
            rule.isSteady(after: 0.5),
            rule.isSteady(after: 0.5),
            rule.isSteady(after: 0.5)
        ]

        #expect(run == [false, false, false, false, true])
    }

    /// With no baseline yet, the first three intervals are sampled rather than counted as steady.
    @Test
    func intervalsSampleTheBaselineBeforeCounting() {
        var rule = SteadyFrameRule()

        let sampling = [rule.isSteady(after: 0.5), rule.isSteady(after: 0.5), rule.isSteady(after: 0.5)]
        let counting = [rule.isSteady(after: 0.5), rule.isSteady(after: 0.5), rule.isSteady(after: 0.5)]

        #expect(sampling == [false, false, false])
        #expect(counting == [false, false, true])
    }

    private func ruleSampled(atBaseline baseline: TimeInterval) -> SteadyFrameRule {
        var rule = SteadyFrameRule()
        _ = rule.sample(baseline)
        _ = rule.sample(baseline)
        _ = rule.sample(baseline)
        return rule
    }
}
