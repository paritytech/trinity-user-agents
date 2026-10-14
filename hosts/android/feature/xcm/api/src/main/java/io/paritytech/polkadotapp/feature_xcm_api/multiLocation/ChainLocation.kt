package io.paritytech.polkadotapp.feature_xcm_api.multiLocation

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

data class ChainLocation(
    val chainId: ChainId,
    val location: AbsoluteMultiLocation
)
