package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.JsEventDispatcher
import kotlinx.coroutines.flow.Flow

/**
 * A booted product worker that the chat surface drives: it delivers user messages, renders custom
 * messages, and dispatches UI events. Keep-alive consumers never touch this; they only hold a
 * reference to keep it alive.
 */
interface ProductWorker : JsEventDispatcher {
    suspend fun onUserMessage(text: String): Result<Unit>

    fun renderMessage(
        chatId: ChatId,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>>
}
