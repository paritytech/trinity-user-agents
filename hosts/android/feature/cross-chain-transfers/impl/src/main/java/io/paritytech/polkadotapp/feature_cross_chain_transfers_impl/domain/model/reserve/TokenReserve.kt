package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve

import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation

internal class TokenReserve(
    val reserveChainLocation: ChainLocation,
    val tokenLocation: AbsoluteMultiLocation
)
