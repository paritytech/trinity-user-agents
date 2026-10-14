package io.paritytech.polkadotapp.feature_tokens_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay

interface AssetDisplayMapper {
    fun displayOf(asset: Chain.Asset): AssetDisplay?
}

fun AssetDisplayMapper.requireDisplayOf(asset: Chain.Asset): AssetDisplay =
    requireNotNull(displayOf(asset)) {
        "Cannot find appearance for ${asset.symbol}"
    }
