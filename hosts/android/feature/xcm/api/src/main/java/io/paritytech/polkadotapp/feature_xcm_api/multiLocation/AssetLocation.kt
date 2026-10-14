package io.paritytech.polkadotapp.feature_xcm_api.multiLocation

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetId

class AssetLocation(
    val assetId: FullChainAssetId,
    val location: AbsoluteMultiLocation
)

fun AssetLocation.multiAssetIdOn(chainLocation: ChainLocation): MultiAssetId {
    val relativeMultiLocation = location.fromPointOfViewOf(chainLocation.location)
    return MultiAssetId(relativeMultiLocation)
}
