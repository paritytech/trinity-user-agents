package io.paritytech.polkadotapp.feature_transfers_api.data.type

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain

interface TokenTransfersTypeRegistry {
    suspend fun typeFor(chainAsset: Chain.Asset): TokenTransfersType
}
