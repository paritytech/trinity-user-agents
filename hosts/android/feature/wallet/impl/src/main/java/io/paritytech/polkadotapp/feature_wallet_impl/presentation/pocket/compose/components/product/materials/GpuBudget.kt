package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

internal class GpuBudget {
    private val frameTimes = mutableListOf<Duration>()

    val timing: Boolean get() = frameTimes.size < TIMED_FRAMES

    // The first frame warms the driver up, so only the second and third decide.
    fun record(frameTime: Duration): Boolean {
        if (!timing) return false
        frameTimes += frameTime
        return frameTimes.size == TIMED_FRAMES && frameTimes.drop(1).all { it > BUDGET }
    }

    fun reset() = frameTimes.clear()

    private companion object {
        const val TIMED_FRAMES = 3
        val BUDGET = 30.milliseconds
    }
}
