package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

class PreimageHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<PreimageLookupParams, PreimageLookupResult>("preimageLookup") { params ->
            botApi.lookupPreimage(params.hash.fromHex()).map {
                PreimageLookupResult(data = it.toHexString(withPrefix = true))
            }
        }

        bridge.registerHandler<PreimageSubmitParams, PreimageSubmitResult>("preimageSubmit") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            botApi.submitPreimage(productId, params.data.fromHex()).map {
                PreimageSubmitResult(hash = it)
            }
        }
    }
}

private data class PreimageLookupParams(val hash: HexString)
private data class PreimageLookupResult(val data: HexString)
private data class PreimageSubmitParams(val data: HexString)
private data class PreimageSubmitResult(val hash: HexString)
