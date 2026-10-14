package io.paritytech.polkadotapp.feature_tokens_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.isUtilityAsset
import io.paritytech.polkadotapp.feature_tokens_api.domain.AssetDisplayMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplayId
import javax.inject.Inject

class RealAssetDisplayMapper @Inject constructor(
    private val knownChains: KnownChains,
) : AssetDisplayMapper {
    override fun displayOf(asset: Chain.Asset): AssetDisplay? {
        return when {
            asset.chainId == knownChains.assetHub && asset.symbol == "USDT" ->
                AssetDisplayId.USDT.intoDisplay(asset)

            asset.chainId == knownChains.assetHub && asset.symbol == "USDC" ->
                AssetDisplayId.USDC.intoDisplay(asset)

            asset.chainId == knownChains.assetHub && asset.isUtilityAsset ->
                AssetDisplayId.DOT.intoDisplay(asset)

            asset.chainId == knownChains.people && asset.isUtilityAsset ->
                AssetDisplayId.DOT.intoDisplay(asset)

            asset.chainId == knownChains.people && asset.isUtilityAsset ->
                AssetDisplayId.DOT.intoDisplay(asset)

            asset.chainId == knownChains.assetHub && asset.symbol == "PAS" ->
                AssetDisplayId.PAS.intoDisplay(asset)

            else -> null
        }
    }

    private fun AssetDisplayId.intoDisplay(asset: Chain.Asset): AssetDisplay {
        return AssetDisplay(asset, this)
    }
}
