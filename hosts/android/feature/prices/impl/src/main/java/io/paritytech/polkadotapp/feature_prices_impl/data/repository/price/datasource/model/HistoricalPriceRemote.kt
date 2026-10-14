package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import java.math.BigDecimal

internal class HistoricalPriceRemote(
    val assetId: FullChainAssetId,
    val price: BigDecimal?,
    val timestamp: Timestamp,
)
