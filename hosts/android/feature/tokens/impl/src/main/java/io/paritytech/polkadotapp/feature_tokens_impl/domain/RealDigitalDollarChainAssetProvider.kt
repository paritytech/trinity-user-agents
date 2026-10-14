package io.paritytech.polkadotapp.feature_tokens_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetId
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

class RealDigitalDollarChainAssetProvider @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val testnetEnvironment: TestnetEnvironment,
) : ChainAssetProvider {
    companion object {
        private const val PUSD_PEOPLE_NIGHTLY = 1
        private const val PUSD_PEOPLE_PRODUCTION = 1
        private const val HOLLAR_PEOPLE_UNSTABLE = 65
    }

    override fun chainId() = knownChains.people

    override suspend fun chain() = chainRegistry.getChain(chainId())

    override suspend fun asset(): Chain.Asset {
        val chain = chain()
        val assetId = assetId()
        val asset = chain.assetsById[assetId]

        return requireNotNull(asset) {
            "Cannot find asset asset in ${chain.name} with id: $assetId"
        }
    }

    override suspend operator fun invoke() = ChainWithAsset(chain = chain(), asset = asset())

    private fun assetId(): ChainAssetId {
        return when (testnetEnvironment) {
            TestnetEnvironment.NIGHTLY -> PUSD_PEOPLE_NIGHTLY
            TestnetEnvironment.PRODUCTION -> PUSD_PEOPLE_PRODUCTION
            TestnetEnvironment.TESTNET -> HOLLAR_PEOPLE_UNSTABLE
        }
    }
}
