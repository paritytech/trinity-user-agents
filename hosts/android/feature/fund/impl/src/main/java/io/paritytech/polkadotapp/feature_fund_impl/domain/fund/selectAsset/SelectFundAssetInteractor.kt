package io.paritytech.polkadotapp.feature_fund_impl.domain.fund.selectAsset

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import javax.inject.Inject

class SelectFundAssetInteractor @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
) {
    suspend fun depositAssets(): List<Chain.Asset> {
        return buildList {
            val chain = chainRegistry.getChain(knownChains.assetHub)
            chain.assetsById[0]?.let(::add)
            chain.assetsById[1]?.let(::add)
            chain.assetsById[2]?.let(::add)
        }
    }
}
