package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

class ChainHostCalls(
    private val botApi: ProductsBotApi,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<ChainNodesParams, List<String>>("chainNodes") { params ->
            botApi.chainNodes(params.genesisHash.hexToDataByteArray())
        }

        bridge.registerHandler<ChainSupportedParams, Boolean>("chainSupported") { params ->
            botApi.chainSupported(params.genesisHash.hexToDataByteArray())
        }
    }
}

private data class ChainNodesParams(val genesisHash: HexString)
private data class ChainSupportedParams(val genesisHash: HexString)
