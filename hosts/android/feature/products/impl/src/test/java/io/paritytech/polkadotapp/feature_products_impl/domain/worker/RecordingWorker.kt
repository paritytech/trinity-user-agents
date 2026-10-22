package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

class RecordingWorker : ProductWorker {
    val delivered = mutableListOf<Pair<ProductChatIdParameter?, String>>()
    val renderedRooms = mutableListOf<ProductChatIdParameter?>()
    val events = mutableListOf<JsUiEvent>()
    val roomCalls = mutableListOf<Pair<String, ProductChatIdParameter>>()
    var briefHolds = 0
    val userActions = mutableListOf<ChatUserActionModel>()

    override suspend fun userAction(roomId: ProductChatIdParameter, action: ChatUserActionModel): Result<ChatUserActionOutcome> {
        userActions += action
        return Result.success(ChatUserActionOutcome.Acknowledged)
    }

    override suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> {
        roomCalls += "open" to roomId
        return Result.success(Unit)
    }

    override suspend fun closeRoom(roomId: ProductChatIdParameter) {
        roomCalls += "close" to roomId
    }

    override fun holdBriefly() {
        briefHolds++
    }

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
