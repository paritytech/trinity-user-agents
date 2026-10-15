package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
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

    /**
     * Gives one event of a product room to the product. A runtime without Chat Extension v2 takes a posted text
     * as a user message and refuses the other events.
     */
    suspend fun publishRoomEvent(roomId: ProductChatIdParameter, event: ProductRoomEvent): Result<Unit> {
        val content = (event as? ProductRoomEvent.MessagePosted)?.content as? ChatMessage.Content.Text
            ?: return Result.failure(UnsupportedRoomEventException(event))
        return onUserMessage(roomId, content.text)
    }

    /** Holds the worker while a room screen shows [roomId], and tells the product. Each success gets one [closeRoom]. */
    suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> = Result.success(Unit)

    suspend fun closeRoom(roomId: ProductChatIdParameter) = Unit

    /** Keeps the worker for a short while without an open room, for example so a new product can create its rooms. */
    fun holdBriefly() = Unit

    suspend fun messagesPage(
        roomId: ProductChatIdParameter,
        anchor: ChatPageAnchorModel,
        pageSize: Int,
    ): Result<ProductMessagesPage> = Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun userAction(roomId: ProductChatIdParameter, action: ChatUserActionModel): Result<ChatUserActionOutcome> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** The menu of [message], whose ids are product ids (`message_actions_query`). */
    suspend fun messageMenu(roomId: ProductChatIdParameter, message: ProductRoomMessage): Result<ChatMessageMenuModel> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantsCount(roomId: ProductChatIdParameter): Result<Int> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantsPage(roomId: ProductChatIdParameter, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantsByIds(roomId: ProductChatIdParameter, participantIds: List<String>): Result<List<ChatParticipantEntryModel>> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>>
}
