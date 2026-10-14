package io.paritytech.polkadotapp.feature_usernames_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import javax.inject.Inject

interface UsernamesChainProvider {
    val chainId: ChainId

    suspend fun chain(): Chain
}

class RealUsernamesChainProvider @Inject constructor(
    knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
) : UsernamesChainProvider {
    override val chainId: ChainId = knownChains.assetHub

    override suspend fun chain(): Chain = chainRegistry.getChain(chainId)
}
