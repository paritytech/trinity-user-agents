package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
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
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow

/**
 * A stable [ProductWorker] the message renderer can hold from construction. It resolves to the
 * shared worker once chat acquires it for driving, so a custom message queued before boot completes
 * still renders against the real instance.
 */
class DeferredProductWorker : ProductWorker {
    private val delegate = MutableStateFlow<Result<ProductWorker>?>(null)

    fun attach(worker: ProductWorker) {
        delegate.value = Result.success(worker)
    }

    fun fail(cause: Throwable) {
        delegate.value = Result.failure(cause)
    }

    private suspend fun awaitWorker(): Result<ProductWorker> = delegate.filterNotNull().first()

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> =
        awaitWorker().fold(
            onSuccess = { it.onUserMessage(roomId, text) },
            onFailure = { Result.failure(it) },
        )

    override suspend fun publishRoomEvent(roomId: ProductChatIdParameter, event: ProductRoomEvent): Result<Unit> =
        awaitWorker().fold(
            onSuccess = { it.publishRoomEvent(roomId, event) },
            onFailure = { Result.failure(it) },
        )

    override suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> =
        awaitWorker().fold(
            onSuccess = { it.openRoom(roomId) },
            onFailure = { Result.failure(it) },
        )

    override suspend fun closeRoom(roomId: ProductChatIdParameter) {
        awaitWorker().getOrNull()?.closeRoom(roomId)
    }

    override fun holdBriefly() {
        delegate.value?.getOrNull()?.holdBriefly()
    }

    override suspend fun messagesPage(
        roomId: ProductChatIdParameter,
        anchor: ChatPageAnchorModel,
        pageSize: Int,
    ): Result<ProductMessagesPage> = awaitWorker().fold(
        onSuccess = { it.messagesPage(roomId, anchor, pageSize) },
        onFailure = { Result.failure(ProductChatQueryException.Unavailable(it)) },
    )

    override suspend fun userAction(roomId: ProductChatIdParameter, action: ChatUserActionModel): Result<ChatUserActionOutcome> =
        awaitWorker().fold(
            onSuccess = { it.userAction(roomId, action) },
            onFailure = { Result.failure(ProductChatQueryException.Unavailable(it)) },
        )

    override suspend fun messageMenu(roomId: ProductChatIdParameter, message: ProductRoomMessage): Result<ChatMessageMenuModel> =
        awaitWorker().fold(
            onSuccess = { it.messageMenu(roomId, message) },
            onFailure = { Result.failure(ProductChatQueryException.Unavailable(it)) },
        )

    override suspend fun participantsCount(roomId: ProductChatIdParameter): Result<Int> =
        withWorker { it.participantsCount(roomId) }

    override suspend fun participantsPage(roomId: ProductChatIdParameter, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> =
        withWorker { it.participantsPage(roomId, cursor, pageSize) }

    override suspend fun participantsByIds(roomId: ProductChatIdParameter, participantIds: List<String>): Result<List<ChatParticipantEntryModel>> =
        withWorker { it.participantsByIds(roomId, participantIds) }

    override suspend fun reactions(roomId: ProductChatIdParameter, messageId: String): Result<List<ChatReactionDetailModel>> =
        withWorker { it.reactions(roomId, messageId) }

    override suspend fun participantMenu(roomId: ProductChatIdParameter, entry: ChatParticipantEntryModel): Result<ChatParticipantMenuModel> =
        withWorker { it.participantMenu(roomId, entry) }

    private suspend fun <T> withWorker(query: suspend (ProductWorker) -> Result<T>): Result<T> = awaitWorker().fold(
        onSuccess = { query(it) },
        onFailure = { Result.failure(ProductChatQueryException.Unavailable(it)) },
    )

    override fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> = flow {
        awaitWorker().fold(
            onSuccess = { emitAll(it.renderMessage(roomId, messageId, messageType, messageData)) },
            onFailure = { emit(Result.failure(it)) },
        )
    }

    override fun renderFooter(roomId: ProductChatIdParameter): Flow<Result<JsWidget>> = flow {
        awaitWorker().fold(
            onSuccess = { emitAll(it.renderFooter(roomId)) },
            onFailure = { emit(Result.failure(it)) },
        )
    }

    override fun dispatchFooterEvent(roomId: ProductChatIdParameter, actionId: String, eventType: JsUiEvent.Type) {
        delegate.value?.getOrNull()?.dispatchFooterEvent(roomId, actionId, eventType)
    }

    // UI events only originate from already-rendered widgets, so the worker is attached by then.
    override fun dispatchEvent(event: JsUiEvent) {
        delegate.value?.getOrNull()?.dispatchEvent(event)
    }
}
