package io.paritytech.polkadotapp.chains.call

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.hasDetectedViewFunction

interface MultiChainViewFunctionsApi {
    suspend fun forChain(chainId: ChainId): ViewFunctionsApi

    suspend fun isSupported(chainId: ChainId, pallet: String, name: String): Boolean
}

internal class RealMultiChainViewFunctionsApi(
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
) : MultiChainViewFunctionsApi {
    override suspend fun forChain(chainId: ChainId): ViewFunctionsApi {
        return RealViewFunctionsApi(multiChainRuntimeCallsApi.forChain(chainId))
    }

    override suspend fun isSupported(
        chainId: ChainId,
        pallet: String,
        name: String
    ): Boolean {
        val runtime = multiChainRuntimeCallsApi.forChain(chainId).runtime
        return runtime.metadata.hasDetectedViewFunction(pallet, name)
    }
}
