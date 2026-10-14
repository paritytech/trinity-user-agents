package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

class EntropyHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<DeriveEntropyParams, DeriveEntropyResponse>("deriveEntropy") { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            val key = params.key.fromHex()

            botApi.deriveEntropy(productId, key).map { entropy ->
                DeriveEntropyResponse(entropy = entropy.toHexString(withPrefix = true))
            }
        }
    }
}

private data class DeriveEntropyParams(val key: HexString)
private data class DeriveEntropyResponse(val entropy: HexString)
