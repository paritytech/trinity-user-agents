package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

/**
 * Sends the room events of the Chat Extension v2 additions to a product through the core. The core keeps the newest
 * events of a product until its worker subscribes, so a publish never starts the worker.
 */
interface ProductChatRoomEventSink {
    suspend fun publish(productId: ProductId, room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit>
}

class TrUAPIChatRoomEventSink @Inject constructor(
    // Lazy: the runtime provider reaches the worker supervisor, which reaches the chat extensions.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
) : ProductChatRoomEventSink {
    override suspend fun publish(productId: ProductId, room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit> =
        runtimeProvider.get().runtime().flatMap { runtime ->
            runCancellableCatching { runtime.publishChatRoomEvent(productId.value, room.value, event.toCore()) }
        }
}
