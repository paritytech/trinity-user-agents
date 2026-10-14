package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility

/**
 * Turns the three numbers that describe one recycler into the two fungibility percentages the UI draws.
 *
 * The inputs are read from three different places and are not guaranteed to agree with each other, so both
 * entry points normalise before dividing: an unloaded count above the included count is clamped down to it,
 * and an empty ring answers zero instead of dividing. With that done, both results provably land in
 * `0..100` whenever `included <= ringCapacity`.
 */
object RecyclerFungibilityCalculator {
    /**
     * The anonymity a voucher gets by entering this ring, measured against the ring's full capacity rather
     * than how full it is right now — this is the ceiling the ring is filling towards.
     *
     * `round(100 * (L^2 - U^2) / L^2)`
     */
    fun maxFungibility(ringCapacity: Int, included: Int, unloaded: Int): RecyclerFungibility {
        return fungibilityOf(ringCapacity, included, unloaded) { capacity, _, clampedUnloaded ->
            val squaredCapacity = capacity * capacity

            Fraction(
                numerator = PERCENT * (squaredCapacity - clampedUnloaded * clampedUnloaded),
                denominator = squaredCapacity
            )
        }
    }

    /**
     * The anonymity the voucher actually has today: the keys already baked into the ring root, discounted by
     * the ones that have been unloaded, and expressed against the ring's capacity.
     *
     * `round(100 * (I^2 - U^2) / (I * L))`
     */
    fun fungibility(ringCapacity: Int, included: Int, unloaded: Int): RecyclerFungibility {
        return fungibilityOf(ringCapacity, included, unloaded) { capacity, clampedIncluded, clampedUnloaded ->
            Fraction(
                numerator = PERCENT * (clampedIncluded * clampedIncluded - clampedUnloaded * clampedUnloaded),
                denominator = clampedIncluded * capacity
            )
        }
    }

    private inline fun fungibilityOf(
        ringCapacity: Int,
        included: Int,
        unloaded: Int,
        fraction: (capacity: Long, included: Long, unloaded: Long) -> Fraction,
    ): RecyclerFungibility {
        // An empty ring hides nothing, and a capacity of zero would divide by zero. Both answer NONE rather
        // than being treated as a bad read, because both are states the chain legitimately reports.
        if (included <= 0 || ringCapacity <= 0) return RecyclerFungibility.NONE

        val clampedUnloaded = unloaded.coerceIn(0, included)
        val result = fraction(ringCapacity.toLong(), included.toLong(), clampedUnloaded.toLong())

        return RecyclerFungibility.ofPercent(result.roundHalfUp())
    }

    private class Fraction(val numerator: Long, val denominator: Long) {
        /**
         * Half-up for non-negative integer fractions without going through `BigDecimal`:
         * `floor((2n + d) / 2d)` shifts the tie up, which `n / d` on its own would truncate down.
         */
        fun roundHalfUp(): Int = ((2 * numerator + denominator) / (2 * denominator)).toInt()
    }
}

private const val PERCENT = 100L
