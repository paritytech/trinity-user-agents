package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge

class QuotedPath<E>(
    val direction: SwapDirection,
    val path: Path<QuotedEdge<E>>,
    val roughFeeEstimation: PathRoughFeeEstimation,
) : Comparable<QuotedPath<E>> {
    private val amountOutAfterFees: Balance = lastSegmentQuote - roughFeeEstimation.inAssetOut
    private val amountInAfterFees: Balance = firstSegmentQuote + roughFeeEstimation.inAssetIn

    override fun compareTo(other: QuotedPath<E>): Int {
        return when (direction) {
            // When we want to sell a token, the bigger the quote - the better
            SwapDirection.SPECIFIED_IN -> (amountOutAfterFees - other.amountOutAfterFees).signum()
            // When we want to buy a token, the smaller the quote - the better
            SwapDirection.SPECIFIED_OUT -> (other.amountInAfterFees - amountInAfterFees).signum()
        }
    }
}

class WeightBreakdown private constructor(
    val individualWeights: List<Int>,
    val total: Int
) {
    companion object {
        fun <N, E : WeightedEdge<N>> fromQuotedPath(path: QuotedPath<E>): WeightBreakdown {
            val weightedPath = mutableListOf<E>()
            val individualWeights = mutableListOf<Int>()
            var weight = 0

            path.path.forEach { quotedEdge ->
                val edgeWeight = quotedEdge.edge.weightForAppendingTo(weightedPath)

                weight += edgeWeight
                weightedPath += quotedEdge.edge
                individualWeights += edgeWeight
            }

            return WeightBreakdown(individualWeights, weight)
        }
    }
}

val QuotedPath<*>.quote: Balance
    get() = when (direction) {
        SwapDirection.SPECIFIED_IN -> lastSegmentQuote
        SwapDirection.SPECIFIED_OUT -> firstSegmentQuote
    }

val QuotedPath<*>.quotedAmount: Balance
    get() = when (direction) {
        SwapDirection.SPECIFIED_IN -> firstSegmentQuotedAmount
        SwapDirection.SPECIFIED_OUT -> lastSegmentQuotedAmount
    }

val QuotedPath<*>.lastSegmentQuotedAmount: Balance
    get() = path.last().quotedAmount

val QuotedPath<*>.lastSegmentQuote: Balance
    get() = path.last().quote

val QuotedPath<*>.firstSegmentQuote: Balance
    get() = path.first().quote

val QuotedPath<*>.firstSegmentQuotedAmount: Balance
    get() = path.first().quotedAmount
