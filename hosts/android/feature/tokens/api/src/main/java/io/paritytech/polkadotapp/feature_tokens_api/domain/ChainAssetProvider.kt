package io.paritytech.polkadotapp.feature_tokens_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

interface ChainAssetProvider {
    fun chainId(): ChainId
    suspend fun chain(): Chain
    suspend fun asset(): Chain.Asset
    suspend operator fun invoke(): ChainWithAsset
}
