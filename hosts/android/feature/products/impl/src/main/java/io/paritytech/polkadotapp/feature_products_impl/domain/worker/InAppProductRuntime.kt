package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatRoomEventSink
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIChatRoomEventSink
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

/**
 * A chat product that the app serves itself in place of a worker. The debug build type binds the chat showcase;
 * every other build binds none.
 */
interface InAppProductRuntime {
    val productId: ProductId

    fun boot(botApi: BindableProductsBotApi, scope: CoroutineScope): ProductWorker

    suspend fun publishRoomEvent(room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit>
}

// The core does not know an in-app product, so the room events of its rooms go to its runtime instead.
class InAppRoutingChatRoomEventSink @Inject constructor(
    private val inAppRuntimes: Set<@JvmSuppressWildcards InAppProductRuntime>,
    private val core: TrUAPIChatRoomEventSink,
) : ProductChatRoomEventSink {
    override suspend fun publish(productId: ProductId, room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit> =
        inAppRuntimes.firstOrNull { it.productId == productId }?.publishRoomEvent(room, event)
            ?: core.publish(productId, room, event)
}
