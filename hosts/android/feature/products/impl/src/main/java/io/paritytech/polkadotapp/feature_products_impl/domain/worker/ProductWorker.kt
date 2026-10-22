package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageRevisionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.JsEventDispatcher
import kotlinx.coroutines.flow.Flow

/**
 * A booted product worker that the chat surface drives: it delivers user messages, renders custom
 * messages, and dispatches UI events. Keep-alive consumers never touch this; they only hold a
 * reference to keep it alive.
 */
interface ProductWorker : JsEventDispatcher {
    suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit>

    /** Holds the worker while a room screen shows [roomId], and tells the product. Each success gets one [closeRoom]. */
    suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> = Result.success(Unit)

    suspend fun closeRoom(roomId: ProductChatIdParameter) = Unit

    /**
     * Wakes the worker without an open room, so a new product can create its rooms (W2 of 07-LATER §10.2). The wake
     * continues until the worker runs, and 30 s more.
     */
    fun holdBriefly() = Unit

    suspend fun messagesPage(
        roomId: ProductChatIdParameter,
        anchor: ChatPageAnchorModel,
        pageSize: Int,
    ): Result<ProductMessagesPage> = Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun userAction(roomId: ProductChatIdParameter, action: ChatUserActionModel): Result<ChatUserActionOutcome> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** The texts that the message with the product id [messageId] had, oldest first (`edit_history`). */
    suspend fun editHistory(roomId: ProductChatIdParameter, messageId: String): Result<List<ChatMessageRevisionModel>> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** The room to show for a link to [roomId] that carries [payload] (`open_link`); null keeps [roomId]. */
    suspend fun openLink(roomId: ProductChatIdParameter, payload: String): Result<ProductChatIdParameter?> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** One page of the messages of [roomId] that contain [text], the newest first (`search_query`). */
    suspend fun search(roomId: ProductChatIdParameter, text: String, cursor: String?, pageSize: Int): Result<ProductMessagesPage> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>>
}
