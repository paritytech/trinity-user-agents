package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatReactionDetailModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductSearchPage
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatShowcase.Participant
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatShowcase.Room
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCount
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toModel
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toPage
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toQueryMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toResolved
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.truapi.ChatParticipantEntry
import uniffi.truapi.ProductChatParticipantsQueryItem
import java.util.concurrent.ConcurrentHashMap

/**
 * The worker of [MockChatProduct]: it answers each query of the room screen from memory, as the product worker of a
 * room that the product keeps would, through the same mappers as the core worker.
 */
class MockChatWorker(
    productId: ProductId,
    private val host: MockChatHost,
    private val botApi: BindableProductsBotApi,
    private val mediaFactory: MockChatMediaFactory,
    private val scope: CoroutineScope,
) : ProductWorker {
    private val rendererId = productId.toChatExtensionId()
    private val ready = CompletableDeferred<MockChatScripts>()
    private val bodies = ConcurrentHashMap<String, MutableStateFlow<MockChatBody>>()

    init {
        // The rooms are declared once the chat surface is bound: every write before fails as unsupported.
        scope.launch {
            botApi.chatSlot.bound.filterNotNull().first()
            val media = withContext(Dispatchers.Default) { mediaFactory.make() }
            val scripts = MockChatScripts(MockChatState(media, now()), host, scope, bodies)
            scripts.seed()
            ready.complete(scripts)
        }
    }

    suspend fun receive(room: ProductChatIdParameter, event: ChatRoomEventModel) {
        ready.await().onRoomEvent(room.value, event)
    }

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> {
        if (roomId != null) ready.await().launchAnswerStored(text, roomId.value)
        return Result.success(Unit)
    }

    override suspend fun publishRoomEvent(roomId: ProductChatIdParameter, event: ProductRoomEvent): Result<Unit> {
        ready.await().onEvent(roomId.value, event)
        return Result.success(Unit)
    }

    override suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> {
        ready.await()
        return Result.success(Unit)
    }

    override suspend fun closeRoom(roomId: ProductChatIdParameter) {
        ready.await().closeRoom(roomId.value)
    }

    override suspend fun messagesPage(roomId: ProductChatIdParameter, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage> =
        showcase(roomId) { scripts -> scripts.state.read { it.page(anchor.toCore(), pageSize) }.toModel(rendererId) }

    override suspend fun userAction(roomId: ProductChatIdParameter, action: ChatUserActionModel): Result<ChatUserActionOutcome> {
        val coreAction = action.toCore()
            ?: return Result.failure(ProductChatQueryException.Failed("the message has no body that the product can take"))
        return showcaseResult(roomId) { scripts -> scripts.userAction(coreAction).map { it.toModel(rendererId) } }
    }

    override suspend fun messageMenu(roomId: ProductChatIdParameter, message: ProductRoomMessage): Result<ChatMessageMenuModel> =
        Result.success(ready.await().menus.messageMenu(message.toQueryMessage()).toModel())

    override suspend fun participantsCount(roomId: ProductChatIdParameter): Result<Int> =
        showcase(roomId) { ProductChatParticipantsQueryItem.Count(participants().size.toUInt()).toCount() }

    override suspend fun participantsPage(roomId: ProductChatIdParameter, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> =
        showcase(roomId) {
            val all = participants()
            val start = cursor?.let { cursor -> all.indexOfFirst { it.participantId == cursor } + 1 } ?: 0
            val end = minOf(start + pageSize, all.size)
            val page = if (start < end) all.subList(start, end) else emptyList()
            ProductChatParticipantsQueryItem.Page(page, next = if (end < all.size) page.lastOrNull()?.participantId else null).toPage()
        }

    override suspend fun participantsByIds(roomId: ProductChatIdParameter, participantIds: List<String>): Result<List<ChatParticipantEntryModel>> =
        showcase(roomId) { ProductChatParticipantsQueryItem.Resolved(participants().filter { it.participantId in participantIds }).toResolved() }

    override suspend fun participantMenu(roomId: ProductChatIdParameter, entry: ChatParticipantEntryModel): Result<ChatParticipantMenuModel> =
        Result.success(ready.await().menus.participantMenu(entry.toCore()).toModel())

    // Who reacted: the user when the summary says so, then named participants for the rest of the count.
    override suspend fun reactions(roomId: ProductChatIdParameter, messageId: String): Result<List<ChatReactionDetailModel>> =
        showcaseResult(roomId) { scripts ->
            val message = scripts.state.read { it.message(messageId) }
                ?: return@showcaseResult Result.failure(ProductChatQueryException.NotAvailable())
            val others = listOf(Participant.ALICE, Participant.BOB, Participant.ASSISTANT) + (1..MockChatShowcase.GUEST_COUNT).map(Participant::guest)
            Result.success(
                message.reactions.flatMap { summary ->
                    val user = if (summary.reactedByUser) listOf(Participant.USER) else emptyList()
                    (user + others.take(maxOf(summary.count.toInt() - user.size, 0))).map { ChatReactionDetailModel(it, summary.emoji) }
                },
            )
        }

    override suspend fun searchPage(roomId: ProductChatIdParameter, text: String, cursor: String?, pageSize: Int): Result<ProductSearchPage> =
        showcase(roomId) { scripts -> scripts.state.read { it.search(text, cursor, pageSize) }.toModel(rendererId) }

    override fun renderMessage(roomId: ProductChatIdParameter?, messageId: ChatMessageId, messageType: String, messageData: DataByteArray): Flow<Result<JsWidget>> {
        val initial = when (messageType) {
            MockChatShowcaseMessages.CustomType.CARD -> MockChatBody.Card(typed = "", primaryPresses = 0)
            MockChatShowcaseMessages.CustomType.COUNTER -> MockChatBody.Counter(0)
            else -> return flowOf(Result.failure(IllegalArgumentException("the showcase draws no $messageType")))
        }
        return draw(bodyKey(roomId, messageId), initial)
    }

    override fun renderFooter(roomId: ProductChatIdParameter): Flow<Result<JsWidget>> = draw(footerKey(roomId.value), MockChatBody.Footer)

    override fun dispatchFooterEvent(roomId: ProductChatIdParameter, actionId: String, eventType: JsUiEvent.Type) {
        scope.launch { ready.await().onFooterAction(roomId.value, actionId) }
    }

    // A press or a change of text inside a body that the product drew. Each stream that shows the body draws it again.
    override fun dispatchEvent(event: JsUiEvent) {
        val body = bodies[bodyKey(event.roomId, event.messageId)] ?: return
        val typed = (event.eventType as? JsUiEvent.Type.InputFieldValueChange)?.newValue
        body.value = body.value.after(event.actionId, typed)
    }

    private fun draw(key: String, initial: MockChatBody): Flow<Result<JsWidget>> =
        bodies.getOrPut(key) { MutableStateFlow(initial) }.map { Result.success(it.tree().toJsWidget()) }

    // A stored message has an id that starts with its room; the product knows it without.
    private fun bodyKey(roomId: ProductChatIdParameter?, messageId: ChatMessageId): String =
        roomId?.let { ProductMessageIds.product(messageId, it.toChatId(rendererId)) } ?: messageId

    private fun footerKey(roomId: String) = "footer/$roomId"

    private suspend fun participants(): List<ChatParticipantEntry> = MockChatShowcase.participants(ready.await().state.media)

    private suspend fun <T> showcase(roomId: ProductChatIdParameter, answer: suspend (MockChatScripts) -> T): Result<T> =
        showcaseResult(roomId) { Result.success(answer(it)) }

    private suspend fun <T> showcaseResult(roomId: ProductChatIdParameter, answer: suspend (MockChatScripts) -> Result<T>): Result<T> {
        val scripts = ready.await()
        if (roomId.value != Room.SHOWCASE) return Result.failure(ProductChatQueryException.NotAvailable())
        return answer(scripts)
    }
}

/** A body that the mock product draws, with the state that a press changes. */
sealed interface MockChatBody {
    data class Card(val typed: String, val primaryPresses: Int) : MockChatBody

    data class Counter(val count: Int) : MockChatBody

    data object Footer : MockChatBody

    fun tree() = when (this) {
        is Card -> MockChatRenderTrees.card(typed, primaryPresses)
        is Counter -> MockChatRenderTrees.counter(count)
        Footer -> MockChatRenderTrees.footer()
    }

    fun after(actionId: String, typed: String?): MockChatBody = when {
        this is Counter && actionId == MockChatRenderTrees.Action.INCREMENT -> copy(count = count + 1)
        this is Counter && actionId == MockChatRenderTrees.Action.RESET -> copy(count = 0)
        this is Card && actionId == MockChatRenderTrees.Action.PRIMARY -> copy(primaryPresses = primaryPresses + 1)
        this is Card && actionId == MockChatRenderTrees.Action.TYPED -> copy(typed = typed.orEmpty())
        else -> this
    }

    fun initial(): MockChatBody = when (this) {
        is Card -> Card(typed = "", primaryPresses = 0)
        is Counter -> Counter(0)
        Footer -> Footer
    }
}

fun now(): ULong = System.currentTimeMillis().toULong()
