package io.paritytech.polkadotapp.feature_pgas_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.getChain
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasChainAssetProvider
import javax.inject.Inject

class RealPgasChainAssetProvider @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
) : PgasChainAssetProvider {
    override fun chainId() = knownChains.assetHub

    override suspend fun chain(): Chain = chainRegistry.getChain(chainId())

    override suspend fun asset(): Chain.Asset {
        val chain = chain()
        val asset = chain.assetsById[PGAS_ASSET_ID_PREVIEWNET]

        return requireNotNull(asset) {
            "Cannot find PGAS asset in ${chain.name} with id: $PGAS_ASSET_ID_PREVIEWNET"
        }
    }

    override suspend operator fun invoke() = ChainWithAsset(chain = chain(), asset = asset())

    private companion object {
        const val PGAS_ASSET_ID_PREVIEWNET: ChainAssetId = 4
    }
}
