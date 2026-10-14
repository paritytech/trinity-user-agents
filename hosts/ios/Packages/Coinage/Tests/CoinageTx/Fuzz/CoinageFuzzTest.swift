import Foundation
import Testing
@testable import Coinage

/// Random walks over coins, vouchers and reorgs, asserting the invariants no reachable state may break.
///
/// Every other durability test checks a case someone thought of. This one is aimed at the cases nobody
/// did: it generates sequences of legal actions and checks only that the ledger never claims something
/// the chain contradicts. A failure is shrunk to the shortest walk that still breaks the invariant and
/// printed as harness calls, so it can become a permanent named test rather than a seed number.
///
/// Eight profiles weight the directions differently, because a uniform walk spends most of its time in
/// the middle of the state space. The defaults keep the run within the normal suite; a long run is
/// `COINAGE_FUZZ_SEEDS=200 COINAGE_FUZZ_STEPS=400`.
@Suite("Coinage Fuzz")
struct CoinageFuzzTest {
    private static let seedsPerProfile = envInt("COINAGE_FUZZ_SEEDS") ?? 3
    private static let steps = envInt("COINAGE_FUZZ_STEPS") ?? 120
    private static let maxShrinkAttempts = 400

    @Test("random walks over coins, vouchers and reorgs keep every durability invariant")
    func randomWalksKeepEveryDurabilityInvariant() async throws {
        // Round robin rather than profile by profile: a run cut short has still sampled every profile.
        for seed in 0 ..< Self.seedsPerProfile {
            for profile in FuzzProfile.all {
                try await runWalk(profile: profile, seed: UInt64(seed))
            }
        }
    }
}

private extension CoinageFuzzTest {
    func runWalk(profile: FuzzProfile, seed: UInt64) async throws {
        guard let failure = try await walkOrNull(profile: profile, seed: seed, replaying: nil) else { return }

        let minimal = try await shrink(failure.trace, profile: profile, seed: seed)

        Issue.record(
            """
            profile '\(profile.name)' seed \(seed) broke an invariant: \(failure.message)
            shrunk from \(failure.trace.count) to \(minimal.count) actions:
            \(minimal.asHarnessCalls())
            """
        )
    }

    /// Returns the violation, or nil when the walk held. Each walk gets a fresh harness.
    func walkOrNull(profile: FuzzProfile, seed: UInt64, replaying: [FuzzAction]?) async throws -> FuzzViolation? {
        let harness = DurabilityHarness()
        harness.givenFuzzSeedAssets()
        let driver = CoinageFuzzDriver(harness: harness)

        do {
            if let replaying {
                try await driver.replay(replaying)
            } else {
                var rng = SplitMix64(seed: seed)
                _ = try await driver.walk(&rng, steps: Self.steps, profile: profile)
            }
            return nil
        } catch let violation as FuzzViolation {
            return violation
        } catch is ReplayDiverged {
            // Only reachable while shrinking: the candidate stopped being a legal history, so it is no
            // evidence either way and the shrinker must keep the action it tried to drop.
            return nil
        }
        // Anything else is the harness itself breaking; letting it propagate fails the test loudly,
        // which is the one outcome a fuzzer must never report as "held".
    }

    /// Delta debugging, coarse to fine: try dropping a large run of actions, halving the size when
    /// nothing more can go. The attempt budget is the backstop — each attempt replays a whole walk.
    func shrink(_ trace: [FuzzAction], profile: FuzzProfile, seed: UInt64) async throws -> [FuzzAction] {
        var best = trace
        var chunk = max(best.count / 2, 1)
        var attempts = 0

        while chunk >= 1, attempts < Self.maxShrinkAttempts {
            var index = 0
            var improved = false

            while index + chunk <= best.count, attempts < Self.maxShrinkAttempts {
                let candidate = Array(best[0 ..< index]) + Array(best[(index + chunk)...])
                attempts += 1

                if try await walkOrNull(profile: profile, seed: seed, replaying: candidate) != nil {
                    best = candidate
                    improved = true
                } else {
                    index += chunk
                }
            }

            if !improved { chunk /= 2 }
        }

        return best
    }
}

private func envInt(_ key: String) -> Int? {
    ProcessInfo.processInfo.environment[key].flatMap(Int.init)
}
