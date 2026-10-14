package io.paritytech.polkadotapp.feature_xcm_impl.converter.asset

import io.novasama.substrate_sdk_android.extensions.tryFindNonNull
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.findAssetByNormalizedSymbol
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.chains.util.normalizeSymbol
import io.paritytech.polkadotapp.feature_xcm_api.config.model.AssetsXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.config.model.ChainAssetReserveId
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.ChainAssetLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.chain.ChainLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation

class RealChainAssetLocationConverter(
    private val xcmConfig: AssetsXcmConfig,
    private val chainLocationConverter: ChainLocationConverter,
    private val chainRegistry: ChainRegistry,
) : ChainAssetLocationConverter {
    private val reserveIdsByLocation = xcmConfig.reservesById.entries.groupBy(
        keySelector = { (_, reserve) -> reserve.tokenLocation },
        valueTransform = { (_, reserve) -> reserve }
    )

    // Association works assuming multiple assets on one chain cannot map to the same reserve
    private val assetIdByReserveIdOverrideAndChain = xcmConfig.assetToReserveIdOverrides.entries.associateBy(
        keySelector = { (assetId, reserveId) -> reserveId to assetId.chainId },
        valueTransform = { (assetId, _) -> assetId.assetId }
    )

    override suspend fun chainAssetFromRelativeLocation(
        location: RelativeMultiLocation,
        pointOfView: Chain
    ): Chain.Asset? {
        val povLocation = chainLocationConverter.absoluteLocationFromChain(pointOfView)
        val assetAbsoluteLocation = location.absoluteLocationViewingFrom(povLocation)

        return findAssetFromReserveLocation(assetAbsoluteLocation, pointOfView)
    }

    override suspend fun absoluteLocationFromChainAsset(chainAsset: Chain.Asset): AbsoluteMultiLocation? {
        val reserveId = getReserveId(chainAsset)
        return xcmConfig.reservesById[reserveId]?.tokenLocation
    }

    override suspend fun relativeLocationFromChainAsset(chainAsset: Chain.Asset): RelativeMultiLocation? {
        val chain = chainRegistry.getChain(chainAsset.chainId)
        val chainLocation = chainLocationConverter.absoluteLocationFromChain(chain)
        val absoluteAssetLocation = absoluteLocationFromChainAsset(chainAsset)
        return absoluteAssetLocation?.fromPointOfViewOf(chainLocation)
    }

    private fun findAssetFromReserveLocation(
        reserveLocation: AbsoluteMultiLocation,
        povChain: Chain,
    ): Chain.Asset? {
        val allMatchingReserves = reserveIdsByLocation[reserveLocation] ?: return null

        return allMatchingReserves.tryFindNonNull { matchingReserve ->
            // We are using povChain id here as we interested in reserve override with the given reserveId that
            // happens on povChain
            val overrideKey = matchingReserve.reserveId to povChain.id
            val overriddenAssetId = assetIdByReserveIdOverrideAndChain[overrideKey]

            if (overriddenAssetId != null) {
                // We found override, this means we can return the relevant asset right away
                povChain.assetsById.getValue(overriddenAssetId)
            } else {
                // If we haven't found an override, it means that either override is not used and reserveId=asset symbol
                // or this reserve is not the right one for asset we are searching for on pov chain
                povChain.findAssetByNormalizedSymbol(matchingReserve.reserveId)
            }
        }
    }

    private fun getReserveId(chainAsset: Chain.Asset): ChainAssetReserveId {
        return xcmConfig.assetToReserveIdOverrides[chainAsset.fullId] ?: chainAsset.normalizeSymbol()
    }
}
