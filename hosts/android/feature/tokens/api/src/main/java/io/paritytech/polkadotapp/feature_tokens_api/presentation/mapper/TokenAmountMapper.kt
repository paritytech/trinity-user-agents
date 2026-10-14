package io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

interface TokenAmountMapper {
    fun mapFrom(source: ChainAssetWithAmount): TokenAmountModel
}
