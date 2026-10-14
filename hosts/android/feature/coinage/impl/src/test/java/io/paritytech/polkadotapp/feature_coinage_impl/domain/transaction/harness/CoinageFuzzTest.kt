package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import kotlinx.coroutines.test.runTest
import org.junit.Assert.fail
import org.junit.Test
import timber.log.Timber
import kotlin.random.Random

/**
 * Random walks over coins, vouchers and reorgs, asserting the invariants no reachable state may break.
 *
 * Every other test in this suite checks a case someone thought of. This one is aimed at the cases nobody
 * did: it generates sequences of legal actions and checks only that the ledger never claims something the
 * chain contradicts. A failure is shrunk to the shortest walk that still breaks the invariant and printed
 * as scenario code, so it can be pasted in as a permanent named test rather than left as a seed number.
 *
 * Eight profiles weight the directions differently, because a uniform walk spends most of its time in
 * the middle of the state space. Restarts, archival and failing reads are rare in the everyday mix and get
 * profiles of their own. 25 seeds each of 300 actions is about 60,000 actions in a few seconds.
 */
class CoinageFuzzTest {
    @Test
    fun `random walks over coins, vouchers and reorgs keep every durability invariant`() {
        // Round robin rather than profile by profile: a run that is cut short, or that fails early, has
        // still sampled every profile rather than only the first one.
        val progress = FuzzProgress(totalWalks = SEEDS_PER_PROFILE * FuzzProfile.ALL.size, stepsPerWalk = STEPS)

        (0 until SEEDS_PER_PROFILE).forEach { seed ->
            FuzzProfile.ALL.forEach { profile ->
                progress.timed(profile, seed.toLong()) { runWalk(profile, seed.toLong(), progress) }
            }
        }
    }

    private fun runWalk(profile: FuzzProfile, seed: Long, progress: FuzzProgress) {
        val failure = walkOrNull(profile, seed) ?: return

        progress.violationFound(profile, seed, failure.trace.size)
        val minimal = shrink(failure.trace) { walkOrNull(profile, seed, replaying = it) != null }

        fail(
            "profile '${profile.name}' seed $seed broke an invariant: ${failure.message}\n" +
                "shrunk from ${failure.trace.size} to ${minimal.size} actions:\n${minimal.asKotlin()}",
        )
    }

    /** Returns the violation, or null when the walk held. Each walk gets a fresh harness. */
    private fun walkOrNull(profile: FuzzProfile, seed: Long, replaying: List<FuzzAction>? = null): FuzzViolation? {
        var violation: FuzzViolation? = null

        runTest {
            // Other suites plant a printing tree globally, and a fuzz run would emit a line per decision.
            Timber.uprootAll()

            val harness = DurabilityHarness(testScope = this, initialState = CoinageChainState.EMPTY)
            harness.givenFuzzSeedAssets()
            val driver = CoinageFuzzDriver(harness)

            runCatching {
                if (replaying != null) driver.replay(replaying) else driver.walk(Random(seed), STEPS, profile)
            }.exceptionOrNull()?.let {
                violation = when (it) {
                    is FuzzViolation -> it

                    // Only reachable while shrinking: the candidate stopped being a legal history, so it is
                    // no evidence either way and the shrinker must keep the action it tried to drop.
                    is ReplayDiverged -> null

                    // Anything else is the harness itself breaking. Swallowing it would report the walk as
                    // held, which is the one outcome a fuzzer must never invent.
                    else -> throw it
                }
            }
        }

        return violation
    }

    /**
     * Delta debugging, coarse to fine: try dropping a large run of actions, halving the size when nothing
     * more can go.
     *
     * Dropping one at a time is what a short trace deserves and what a long one cannot afford — each attempt
     * replays the whole walk, so single-action steps make shrinking grow with the cube of trace length and a
     * deep walk never finishes. The attempt budget is the backstop: a trace that resists shrinking is still
     * reported, just longer than it might have been.
     */
    private fun shrink(trace: List<FuzzAction>, stillViolates: (List<FuzzAction>) -> Boolean): List<FuzzAction> {
        var best = trace
        var chunk = maxOf(best.size / 2, 1)
        var attempts = 0

        while (chunk >= 1 && attempts < MAX_SHRINK_ATTEMPTS) {
            var index = 0
            var improved = false

            while (index + chunk <= best.size && attempts < MAX_SHRINK_ATTEMPTS) {
                val candidate = best.take(index) + best.drop(index + chunk)
                attempts++

                if (stillViolates(candidate)) {
                    best = candidate
                    improved = true
                } else {
                    index += chunk
                }
            }

            if (!improved) chunk /= 2
        }

        return best
    }

    private companion object {
        /**
         * Overridable for a long run: `-Dcoinage.fuzz.seeds=5000 -Dcoinage.fuzz.steps=400`.
         *
         * The defaults keep this within a few seconds so it runs with the normal suite. Raising seeds buys
         * more distinct histories; raising steps buys deeper ones, and only deep walks reach the states that
         * need many entries alive at once.
         */
        val SEEDS_PER_PROFILE = System.getProperty("coinage.fuzz.seeds")?.toInt() ?: 25
        val STEPS = System.getProperty("coinage.fuzz.steps")?.toInt() ?: 300

        /** Bounds the worst case: every attempt replays a whole walk. */
        const val MAX_SHRINK_ATTEMPTS = 400
    }
}

/**
 * Progress on stdout, so a run measured in minutes can be watched rather than guessed at. Gradle only shows
 * it when the run was asked for explicitly — see the `showStandardStreams` clause in the module's build file.
 *
 * The rate is taken over the whole run rather than since the last report: profiles differ in what an action
 * costs and they are round-robined, so an instantaneous rate swings by more than the estimate is worth.
 */
private class FuzzProgress(private val totalWalks: Int, private val stepsPerWalk: Int) {
    private val reportEvery = maxOf(totalWalks / REPORTS_PER_RUN, 1)
    private val startedAt = System.nanoTime()
    private var walksDone = 0
    private var reportedAt = 0L

    /**
     * A single walk can cost orders of magnitude more than its neighbours, and an average over the run hides
     * exactly that: the run looks merely slow rather than stuck on one seed. Slow walks are named as they
     * happen, and the rate is reported both over the run and since the last report.
     */
    fun timed(profile: FuzzProfile, seed: Long, walk: () -> Unit) {
        val walkStartedAt = System.nanoTime()
        walk()
        val walkSeconds = (System.nanoTime() - walkStartedAt) / 1_000_000_000

        if (walkSeconds >= SLOW_WALK_SECONDS) {
            println("fuzz: slow walk — profile '${profile.name}' seed $seed took ${format(walkSeconds)}")
        }

        walksDone++
        if (walksDone % reportEvery != 0 && walksDone != totalWalks) return

        val elapsed = elapsedSeconds()
        val actions = walksDone.toLong() * stepsPerWalk
        val remaining = (totalWalks - walksDone).toLong() * elapsed / walksDone
        val sinceReport = maxOf(elapsed - reportedAt, 1)

        println(
            "fuzz: $walksDone/$totalWalks walks, $actions actions, ${format(elapsed)} elapsed, " +
                "${actions / maxOf(elapsed, 1)} actions/s overall, " +
                "${reportEvery.toLong() * stepsPerWalk / sinceReport} actions/s since last, " +
                "~${format(remaining)} left",
        )
        reportedAt = elapsed
    }

    /** Shrinking replays a whole walk per attempt, so a run can sit here for a while with nothing to show. */
    fun violationFound(profile: FuzzProfile, seed: Long, actions: Int) {
        println("fuzz: profile '${profile.name}' seed $seed broke an invariant after $actions actions; shrinking")
    }

    private fun elapsedSeconds() = (System.nanoTime() - startedAt) / 1_000_000_000

    private fun format(seconds: Long) = when {
        seconds >= 60 -> "${seconds / 60}m${(seconds % 60).toString().padStart(2, '0')}s"
        else -> "${seconds}s"
    }

    private companion object {
        const val REPORTS_PER_RUN = 20

        /** Well above a healthy walk, which runs in milliseconds, and below the stalls worth chasing. */
        const val SLOW_WALK_SECONDS = 2
    }
}
