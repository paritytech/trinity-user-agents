package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.common.utils.Fraction
import java.math.BigDecimal

data class SwapQuote(
    val amountIn: ChainAssetWithAmount,
    val amountOut: ChainAssetWithAmount,
    val priceImpact: Fraction,
    val quotedPath: QuotedPath<SwapGraphEdge>,
    val executionEstimate: SwapExecutionEstimate,
    val direction: SwapDirection,
) {
    val assetIn: Chain.Asset
        get() = amountIn.chainAsset

    val assetOut: Chain.Asset
        get() = amountOut.chainAsset

    val planksIn: Balance
        get() = amountIn.amount

    val planksOut: Balance
        get() = amountOut.amount
}

// Conversion rate from assetIn to assetOut
// In other words 1 unit of assetIn = swapRate() units of asset out
fun SwapQuote.swapRate(): BigDecimal {
    return amountIn rateAgainst amountOut
}

infix fun ChainAssetWithAmount.rateAgainst(assetOut: ChainAssetWithAmount): BigDecimal {
    if (amount == Balance.ZERO) return BigDecimal.ZERO

    val amountIn = chainAsset.amountFromPlanks(amount)
    val amountOut = assetOut.chainAsset.amountFromPlanks(assetOut.amount)

    return amountOut / amountIn
}
