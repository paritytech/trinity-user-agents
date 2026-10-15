package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatReactionDetailModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductSearchPage
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.JsEventDispatcher
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf

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

    /** The menu of [message], whose ids are product ids (`message_actions_query`). */
    suspend fun messageMenu(roomId: ProductChatIdParameter, message: ProductRoomMessage): Result<ChatMessageMenuModel> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantsCount(roomId: ProductChatIdParameter): Result<Int> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantsPage(roomId: ProductChatIdParameter, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantsByIds(roomId: ProductChatIdParameter, participantIds: List<String>): Result<List<ChatParticipantEntryModel>> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** Who reacted to the message with the product id [messageId] (`reactions_query`). */
    suspend fun reactions(roomId: ProductChatIdParameter, messageId: String): Result<List<ChatReactionDetailModel>> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    suspend fun participantMenu(roomId: ProductChatIdParameter, entry: ChatParticipantEntryModel): Result<ChatParticipantMenuModel> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** The messages of [roomId] that match [text], newest first (`search_query`). */
    suspend fun searchPage(roomId: ProductChatIdParameter, text: String, cursor: String?, pageSize: Int): Result<ProductSearchPage> =
        Result.failure(ProductChatQueryException.Unavailable(cause = null))

    /** The tree of the footer that the product draws for [roomId] (`RenderContext::ChatFooter`). */
    fun renderFooter(roomId: ProductChatIdParameter): Flow<Result<JsWidget>> =
        flowOf(Result.failure(UnsupportedOperationException("this runtime draws no chat footer")))

    /** A press or an input in the footer that the product draws. */
    fun dispatchFooterEvent(roomId: ProductChatIdParameter, actionId: String, eventType: JsUiEvent.Type) = Unit

    fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>>
}
