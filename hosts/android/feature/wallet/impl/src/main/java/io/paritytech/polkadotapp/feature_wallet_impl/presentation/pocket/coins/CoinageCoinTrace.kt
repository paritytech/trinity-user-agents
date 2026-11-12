package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

/**
 * What the card says about one holding once it is picked out: what it is worth, whether it is Ready or
 * Clearing, how many hops it has made since it last cleared, and how well it cleared.
 *
 * Deliberately no more than four things, and none of them names a mechanism: what the user is told has to
 * stay plain and defensible. No hop-by-hop history either — received coins carry synthetic hops, so a list
 * would overstate what is known.
 */
@Immutable
data class CoinageCoinTrace(
    val value: TokenAmountModel,
    /**
     * Exactly the block the coin sits in — the user's own classification, preference included — so the
     * detail never contradicts the grid header.
     */
    val partition: CoinageStripLayout.Partition,
    /** Hops since the holding last cleared. Nought for one still in a recycler. */
    val hops: Int,
    val quality: Quality
) {
    /**
     * The clearing score in bands that double in width, so each grade up is as hard again to reach as the
     * last, the same reason the wear is drawn on a doubling ladder. [UNKNOWN] is not a grade: it is a coin
     * with no score at all, which is every coin received from someone else, and nothing is being claimed.
     */
    enum class Quality {
        UNKNOWN,
        VERY_LOW,
        LOW,
        FAIR,
        GOOD,
        HIGH;

        companion object {
            fun of(score: Int?): Quality = when {
                score == null -> UNKNOWN
                score <= 8 -> VERY_LOW
                score <= 16 -> LOW
                score <= 32 -> FAIR
                score <= 64 -> GOOD
                else -> HIGH
            }
        }
    }
}
