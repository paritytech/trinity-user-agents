package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import java.math.BigDecimal

internal class PriceRemote(
    val assetId: FullChainAssetId,
    val price: BigDecimal?
)
