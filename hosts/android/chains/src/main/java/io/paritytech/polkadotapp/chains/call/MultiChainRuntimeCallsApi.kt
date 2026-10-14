package io.paritytech.polkadotapp.chains.call

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.getSocket
import io.paritytech.polkadotapp.chains.util.hasDetectedRuntimeApi

interface MultiChainRuntimeCallsApi {
    suspend fun forChain(chainId: ChainId): RuntimeCallsApi

    suspend fun isSupported(chainId: ChainId, section: String, method: String): Boolean
}

internal class RealMultiChainRuntimeCallsApi(
    private val chainRegistry: ChainRegistry,
) : MultiChainRuntimeCallsApi {
    override suspend fun forChain(chainId: ChainId): RuntimeCallsApi {
        val runtime = chainRegistry.getRuntime(chainId)
        val socket = chainRegistry.getSocket(chainId)

        return RealRuntimeCallsApi(runtime, socket)
    }

    override suspend fun isSupported(
        chainId: ChainId,
        section: String,
        method: String
    ): Boolean {
        val runtime = chainRegistry.getRuntime(chainId)
        return runtime.metadata.hasDetectedRuntimeApi(section, method)
    }
}
