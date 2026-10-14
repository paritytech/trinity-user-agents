package io.paritytech.polkadotapp.feature_xcm_api.config.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation

class ChainAssetReserveConfig(
    val reserveId: ChainAssetReserveId,
    val reserveAssetId: FullChainAssetId,
    val tokenLocation: AbsoluteMultiLocation,
)
