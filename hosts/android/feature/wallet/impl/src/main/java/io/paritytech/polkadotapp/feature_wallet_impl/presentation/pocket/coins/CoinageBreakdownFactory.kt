package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.CoinageHolding
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.holdingKeyOrder

/**
 * Turns classified holdings into the coins the card draws, in the order it draws them.
 *
 * Pure and free of the presenter's state, so the ordering can be exercised directly.
 */
object CoinageBreakdownFactory {
    /**
     * Which half of a denomination's run a row belongs to. Holdings whose recycler we have no record of are
     * the least fungible thing we can say anything about, so they lead.
     */
    private const val UNKNOWN_HISTORY = 0
    private const val KNOWN_LEVEL = 1

    fun coins(holdings: List<CoinageHolding>): List<CoinageScene.Coin> = inDisplayOrder(
        holdings
            .sortedWith(rowOrder)
            .map { holding ->
                val wear = wear(holding.recyclerFungibility, holding.isBatchUnloaded)

                CoinageScene.Coin(
                    id = holding.id,
                    exponent = holding.exponent.value,
                    wear = wear,
                    partition = if (holding.isReady) {
                        CoinageStripLayout.Partition.READY
                    } else {
                        CoinageStripLayout.Partition.CLEARING
                    },
                    level = CoinageWear.level(wear),
                    hops = holding.hops
                )
            }
    )

    /**
     * How worn a holding is drawn, from the size of the crowd its recycler hides it in.
     *
     * The stored score is a share of ring capacity, so it converts back to a crowd size before it goes on the
     * doubling scale the depiction uses. Without a recycler record nothing can be credited, and the holding
     * wears as though it hides among nobody.
     */
    fun wear(score: RecyclerFungibility?, isBatchUnloaded: Boolean): Float {
        if (score == null) return CoinageWear.unknown

        val crowd = (
            score.percent.coerceAtMost(CoinageStatusMetrics.FULL_FUNGIBILITY).toFloat() /
                CoinageStatusMetrics.FULL_FUNGIBILITY *
                (CoinageWear.RING_CAPACITY - 1)
            ).toInt()
        val penalty = if (isBatchUnloaded) CoinageStatusMetrics.BATCH_UNLOAD_PENALTY else 0

        return CoinageWear.amount(maxOf(CoinageWear.level(hiddenAmong = crowd) - penalty, 0))
    }

    /**
     * The whole order coins are drawn in: Clearing before Ready, then largest denomination first, then least
     * fungible first, and deepest history first among those.
     *
     * The first two keys are the stakeholder's and override an earlier instruction that had fungibility
     * leading: with the partitions in place, that put a one-cent coin nobody can trace ahead of the largest
     * coin in the same block, and the eye had nothing to hold on to.
     *
     * Stated over what a [CoinageScene.Coin] carries rather than only over a row, so it holds for any list.
     * Anything assembled without a holding behind it — the test-data switch, which is the only place the
     * depiction is ever reviewed — would otherwise reach the layout in whatever order it was generated in,
     * and both layouts start a new block wherever the partition changes, so that is a block, and a header,
     * per coin.
     *
     * Kotlin's sort is stable, so live holdings are unaffected: they arrive in this order already.
     */
    fun inDisplayOrder(coins: List<CoinageScene.Coin>): List<CoinageScene.Coin> = coins.sortedWith(
        // Clearing first.
        compareBy<CoinageScene.Coin> { it.partition != CoinageStripLayout.Partition.CLEARING }
            // Value is `unit * 2^exponent`, so ordering by exponent is exactly ordering by value.
            .thenByDescending { it.exponent }
            // Level counts the doublings of the crowd a coin hides in, so the lowest hides among fewest and
            // wears hardest.
            .thenBy { it.level }
            .thenByDescending { it.hops }
    )

    /**
     * Within one denomination: unknown histories first, then least fungible, with the derivation index as the
     * last word so equal holdings keep their order across refreshes.
     */
    private val rowOrder: Comparator<CoinageHolding> =
        compareByDescending<CoinageHolding> { it.exponent.value }
            .thenBy { if (it.recyclerFungibility == null) UNKNOWN_HISTORY else KNOWN_LEVEL }
            .thenByDescending { severity(it) }
            .then(holdingKeyOrder)

    /**
     * Descending within a standing: hop count where there is no recycler record, fungibility bucket where
     * there is. Both read "least fungible first".
     */
    private fun severity(holding: CoinageHolding): Int {
        val fungibility = holding.recyclerFungibility ?: return holding.hops

        val base = CoinageStatusMetrics.bucket(fungibility.percent)

        if (!holding.isBatchUnloaded) return base

        return minOf(base + CoinageStatusMetrics.BATCH_UNLOAD_PENALTY, CoinageStatusMetrics.maximumBucket)
    }
}
