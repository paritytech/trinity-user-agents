package io.paritytech.polkadotapp.feature_coinage_api.domain.model

/**
 * How much of a recycler's anonymity is still unspent, as a percentage.
 *
 * `0` reads as "hides in nothing" and `100` as "hides in the whole ring". The two ends are what the UI
 * draws, so a value outside `0..100` would render a bar past the column rather than fail loudly — which is
 * why [ofPercent] is the only way to build one.
 */
@JvmInline
value class RecyclerFungibility private constructor(val percent: Int) : Comparable<RecyclerFungibility> {
    override fun compareTo(other: RecyclerFungibility): Int = percent.compareTo(other.percent)

    companion object {
        /**
         * No anonymity left to draw on. Also the value a voucher carries before its ring is known, since a
         * voucher that is not in a ring yet hides in nothing either.
         */
        val NONE = RecyclerFungibility(MIN_PERCENT)

        /** Clamps into `0..100`; the inputs are independent chain reads that can disagree with each other. */
        fun ofPercent(percent: Int) = RecyclerFungibility(percent.coerceIn(MIN_PERCENT, MAX_PERCENT))
    }
}

/**
 * Top-level rather than in the companion: reading a private companion const from a value class member
 * crashes the Kotlin backend the same way it does for enums (see `RingExponent`).
 */
private const val MIN_PERCENT = 0
private const val MAX_PERCENT = 100
