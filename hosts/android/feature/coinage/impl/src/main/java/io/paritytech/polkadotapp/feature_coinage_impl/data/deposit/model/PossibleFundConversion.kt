package io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapFee
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapQuote

data class PossibleFundConversion(
    val quote: SwapQuote,
    val fee: SwapFee
)

fun PossibleFundConversion.depositedAmount(): ChainAssetWithAmount {
    return quote.amountIn
}

fun PossibleFundConversion.expectedConvertedAmount(): ChainAssetWithAmount {
    return quote.amountOut
}
