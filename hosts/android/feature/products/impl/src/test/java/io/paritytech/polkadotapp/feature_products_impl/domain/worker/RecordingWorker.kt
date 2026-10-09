package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

class RecordingWorker : ProductWorker {
    val delivered = mutableListOf<Pair<ProductChatIdParameter?, String>>()
    val renderedRooms = mutableListOf<ProductChatIdParameter?>()
    val events = mutableListOf<JsUiEvent>()

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> {
        delivered += roomId to text
        return Result.success(Unit)
    }

    override fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> {
        renderedRooms += roomId
        return emptyFlow()
    }

    override fun dispatchEvent(event: JsUiEvent) {
        events += event
    }
}
