package io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import java.math.BigDecimal

interface TokenAmountMapper {
    fun mapFrom(source: ChainAssetWithAmount): TokenAmountModel

    /** A payment-balance amount already in decimal units, shown as the payment asset's brand. */
    fun digitalDollar(amount: BigDecimal): TokenAmountModel
}
