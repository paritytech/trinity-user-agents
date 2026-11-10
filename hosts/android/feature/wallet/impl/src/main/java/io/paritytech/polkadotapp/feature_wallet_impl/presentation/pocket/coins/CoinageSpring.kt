package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.abs
import kotlin.math.exp

/**
 * A critically damped spring, exact for any time step.
 *
 * Every channel of every coin runs one of these, so an arrangement change only moves targets and a tap
 * mid-flight simply re-aims. Ported from `src/layout/springs.js`.
 */
class CoinageSpring(var value: Float = 0f, var velocity: Float = 0f) {
    fun step(target: Float, seconds: Float, omega: Float) {
        val offset = value - target
        val decay = exp(-omega * seconds)
        val travel = (velocity + omega * offset) * seconds

        value = target + (offset + travel) * decay
        velocity = (velocity - omega * travel) * decay
    }

    fun isSettled(target: Float): Boolean = abs(value - target) < EPSILON && abs(velocity) < EPSILON

    companion object {
        /** Below this a channel counts as at rest, and a frame with nothing above it is not drawn. */
        const val EPSILON = 0.0005f

        /**
         * Strip and grid moves stagger their starts across this long, in display order, so the field unfolds
         * rather than jumping as one block.
         */
        const val STAGGER = 0.18f

        /** One step of the spring, as a pair. Separate from the object so the vectors can exercise it. */
        fun step(value: Float, velocity: Float, target: Float, seconds: Float, omega: Float): Pair<Float, Float> {
            val offset = value - target
            val decay = exp(-omega * seconds)
            val travel = (velocity + omega * offset) * seconds

            return (target + (offset + travel) * decay) to ((velocity - omega * travel) * decay)
        }
    }

    /**
     * How fast each channel chases its target. Position moves fastest, wear slowest: a coin changing how
     * hidden it is should ease between levels rather than snap.
     */
    object Omega {
        const val POSITION = 13f
        const val ROTATION = 9f
        const val THICKNESS = 10f
        const val LUSTER = 7f
        const val WEAR = 2.2f
    }
}
