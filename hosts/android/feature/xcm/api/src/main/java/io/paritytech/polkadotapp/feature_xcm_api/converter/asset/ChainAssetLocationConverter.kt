package io.paritytech.polkadotapp.feature_xcm_api.converter.asset

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion

interface ChainAssetLocationConverter {
    /**
     * Find asset on [pointOfView] chain that matches given [location]
     */
    suspend fun chainAssetFromRelativeLocation(
        location: RelativeMultiLocation,
        pointOfView: Chain,
    ): Chain.Asset?

    /**
     * Convert given [chainAsset] to absolute location
     */
    suspend fun absoluteLocationFromChainAsset(chainAsset: Chain.Asset): AbsoluteMultiLocation?

    /**
     * Convert given [chainAsset] to relative location from the pov of its chain
     */
    suspend fun relativeLocationFromChainAsset(chainAsset: Chain.Asset): RelativeMultiLocation?
}

suspend fun ChainAssetLocationConverter.relativeLocationFromChainAssetOrThrow(chainAsset: Chain.Asset): RelativeMultiLocation {
    return requireNotNull(relativeLocationFromChainAsset(chainAsset)) {
        "Cannot convert ${chainAsset.symbol} on ${chainAsset.chainId} to multi-location"
    }
}

suspend fun ChainAssetLocationConverter.encodableMultiLocationOf(
    chainAsset: Chain.Asset,
    xcmVersion: XcmVersion
): Any {
    return relativeLocationFromChainAssetOrThrow(chainAsset).toEncodableInstance(xcmVersion)
}
