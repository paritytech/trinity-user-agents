package io.paritytech.polkadotapp.feature_prices_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId

interface PriceLookup {
    operator fun get(fullChainAssetId: FullChainAssetId): Price
}
