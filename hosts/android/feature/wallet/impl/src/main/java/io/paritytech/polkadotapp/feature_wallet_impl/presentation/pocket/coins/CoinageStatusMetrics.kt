package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

/**
 * The fungibility ladder: how a recycler's score becomes a bucket, and what a bucket is worth.
 *
 * Quantised rather than continuous because the meaning of a difference is not linear, and because discrete
 * steps are what let holdings of the same standing be told apart from ones merely close to each other.
 */
object CoinageStatusMetrics {
    /** A recycler score is a percentage, so this is the top of the scale. */
    const val FULL_FUNGIBILITY = 100

    /**
     * Penalty for a coin unloaded as one of a batch: the batch links it to the others that came out with it,
     * which the recycler's own score does not account for.
     *
     * A flat step rather than `log(batch size)` because the batch size is not recorded. Two buckets is about
     * a two-and-a-third-fold linkage, which understates a typical batch; a deliberate approximation.
     */
    const val BATCH_UNLOAD_PENALTY = 2

    /**
     * Lowest fungibility percentage in each bucket, most fungible first. Ratio is about 1.53 per step, so a
     * bucket is roughly a one-and-a-half-fold change in the anonymity set, with the last two widened because
     * scores that low are rare and not worth separating.
     */
    private val BUCKET_FLOORS = intArrayOf(66, 43, 28, 19, 12, 8, 5, 2, 0)

    /** Buckets run `0` (fully fungible) to [maximumBucket] (no anonymity). */
    val maximumBucket: Int = BUCKET_FLOORS.lastIndex

    /**
     * Buckets a fungibility percentage onto the log-ish ladder.
     *
     * Logarithmic rather than linear because the meaning of a difference is: going from being fungible with
     * one other coin to four is substantial, going from 510 to 511 is not.
     */
    fun bucket(score: Int): Int {
        val clamped = score.coerceAtMost(FULL_FUNGIBILITY)

        return BUCKET_FLOORS.indexOfFirst { clamped >= it }.takeIf { it >= 0 } ?: maximumBucket
    }
}
