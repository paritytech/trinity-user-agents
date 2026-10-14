package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

class UserIdHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<Unit, GetUserIdResponse>("getUserId") {
            botApi.getUserId(callingProductIdProvider.getProductId().getOrThrow())
                .map { GetUserIdResponse(primaryUsername = it.primaryUsername) }
        }
    }
}

private data class GetUserIdResponse(val primaryUsername: String)
