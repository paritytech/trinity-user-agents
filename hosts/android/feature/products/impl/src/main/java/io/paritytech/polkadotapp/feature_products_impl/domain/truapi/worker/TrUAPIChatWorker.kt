package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.ChatQueryException
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
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
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCoreBody
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCount
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toModel
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toPage
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toQueryException
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toQueryMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toResolved
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.ActionTrigger
import uniffi.truapi.ChatActionPayload
import uniffi.truapi.ChatCommand
import uniffi.truapi.ChatEvent
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatParticipantsQuery
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductChatMessageActionsQueryRequest
import uniffi.truapi.ProductChatMessagesQueryRequest
import uniffi.truapi.ProductChatParticipantActionsQueryRequest
import uniffi.truapi.ProductChatParticipantsQueryRequest
import uniffi.truapi.ProductChatReactionsQueryRequest
import uniffi.truapi.ProductChatUserActionRequest
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.ProductRuntimeException
import uniffi.truapi.RenderContext
import java.util.concurrent.ConcurrentHashMap
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class TrUAPIChatWorker(
    private val productId: ProductId,
    private val runtime: TrUAPIHostRuntime,
    private val workers: TrUAPIWorkerSupervisor,
    private val chatMessaging: ProductChatMessaging,
    private val scope: CoroutineScope,
) : ProductWorker {
    private val rendererId = productId.toChatExtensionId()

    // The worker runs while a room screen is open, or briefly for a message in flight; never for the life of
    // the installed integration (D10).
    private val holds = WorkerHolds(
        acquire = { runtime.acquireWorker(productId.value) },
        release = {
            runCatching { runtime.releaseWorker(productId.value) }
                .onFailure { Timber.w(it, "TrUAPI releaseWorker failed for %s", productId.value) }
        },
    )
    private val roomAnnouncements = ConcurrentHashMap<String, Job>()

    init {
        // A coroutine of the scope releases each brief hold. After the scope ends, no coroutine can release a hold,
        // so the holds refuse each new hold.
        scope.coroutineContext.job.invokeOnCompletion { holds.close() }
        scope.launch {
            workers.executionState(productId)
                .map { (it as? WorkerExecutionState.Running)?.execution }
                .distinctUntilChanged()
                .collectLatest { execution ->
                    if (execution == null) return@collectLatest
                    coroutineScope { TrUAPIChatRoomForwarding(productId, execution, chatMessaging).start(this) }
                }
        }
    }

    override suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> {
        runCatching { holds.openRoom(roomId.value) }
            .onFailure { return Result.failure<Unit>(it).logFailure("truapi.chat.open for ${productId.value}") }
        workers.restartIfFailed(productId)
        // The screen does not wait for the product: `RoomOpened` goes once the worker runs.
        roomAnnouncements.put(roomId.value, scope.launch { publishNow(roomId, ChatEvent.RoomOpened) })?.cancel()
        return Result.success(Unit)
    }

    override suspend fun closeRoom(roomId: ProductChatIdParameter) {
        if (!holds.isOpen(roomId.value)) return
        roomAnnouncements.remove(roomId.value)?.cancel()
        workers.currentExecution(productId)?.let { execution ->
            runCatching { execution.publishChatEvent(roomId.value, NATIVE_PEER, ChatEvent.RoomClosed) }
                .logFailure("truapi.chat.close for ${productId.value}")
        }
        holds.closeRoom(roomId.value)
    }

    override fun holdBriefly() {
        runCatching { holds.hold() }.onFailure {
            Timber.w(it, "TrUAPI worker hold failed for %s", productId.value)
            return
        }
        scope.launch {
            try {
                withTimeoutOrNull(TrUAPIWorkerSupervisor.EXECUTION_WAIT_TIMEOUT) { runningExecution() }
                delay(BRIEF_HOLD)
            } finally {
                holds.drop()
            }
        }
    }

    override suspend fun messagesPage(
        roomId: ProductChatIdParameter,
        anchor: ChatPageAnchorModel,
        pageSize: Int,
    ): Result<ProductMessagesPage> = query { execution ->
        execution.chatMessagesQuery(ProductChatMessagesQueryRequest(roomId.value, anchor.toCore(), pageSize.toUInt()))
            .toModel(rendererId)
    }

    override suspend fun userAction(roomId: ProductChatIdParameter, action: ChatUserActionModel): Result<ChatUserActionOutcome> {
        val coreAction = action.toCore()
            ?: return Result.failure(ProductChatQueryException.Failed("the message has no body that the product can take"))
        return query { execution ->
            execution.chatUserAction(ProductChatUserActionRequest(roomId.value, coreAction)).toModel(rendererId)
        }
    }

    override suspend fun messageMenu(roomId: ProductChatIdParameter, message: ProductRoomMessage): Result<ChatMessageMenuModel> =
        query { execution ->
            execution.chatMessageActionsQuery(ProductChatMessageActionsQueryRequest(roomId.value, message.toQueryMessage())).menu.toModel()
        }

    override suspend fun participantsCount(roomId: ProductChatIdParameter): Result<Int> = query { execution ->
        execution.chatParticipantsQuery(ProductChatParticipantsQueryRequest(roomId.value, ChatParticipantsQuery.Count)).toCount()
    }

    override suspend fun participantsPage(roomId: ProductChatIdParameter, cursor: String?, pageSize: Int): Result<ProductParticipantsPage> =
        query { execution ->
            val page = ChatParticipantsQuery.Page(cursor, pageSize.toUInt())
            execution.chatParticipantsQuery(ProductChatParticipantsQueryRequest(roomId.value, page)).toPage()
        }

    override suspend fun participantsByIds(roomId: ProductChatIdParameter, participantIds: List<String>): Result<List<ChatParticipantEntryModel>> =
        query { execution ->
            val byIds = ChatParticipantsQuery.ByIds(participantIds)
            execution.chatParticipantsQuery(ProductChatParticipantsQueryRequest(roomId.value, byIds)).toResolved()
        }

    override suspend fun reactions(roomId: ProductChatIdParameter, messageId: String): Result<List<ChatReactionDetailModel>> =
        query { execution ->
            execution.chatReactionsQuery(ProductChatReactionsQueryRequest(roomId.value, messageId))
                .reactions
                .map { ChatReactionDetailModel(it.participantId, it.emoji) }
        }

    override suspend fun participantMenu(roomId: ProductChatIdParameter, entry: ChatParticipantEntryModel): Result<ChatParticipantMenuModel> =
        query { execution ->
            execution.chatParticipantActionsQuery(ProductChatParticipantActionsQueryRequest(roomId.value, entry.toCore())).menu.toModel()
        }

    // The core holds the worker for each open query. A worker that is not running yet starts first, and an
    // unavailable worker is asked once more after a second (02-DESIGN §3.6, §5.1). The supervisor reports a
    // restarted worker as running before its client connects to the core, so a query waits for that connection
    // for a limited time.
    private suspend fun <T> query(ask: suspend (TrUAPIProductExecution) -> T): Result<T> {
        var attempt = 1
        var connectWaits = 0
        while (true) {
            val outcome = runCatching { ask(awaitRunningExecution()) }
            val failure = outcome.exceptionOrNull() ?: return outcome
            if (failure is CancellationException) throw failure
            if (failure is ProductRuntimeException.NotConnected && connectWaits < CONNECT_WAITS) {
                connectWaits++
                delay(CONNECT_WAIT_STEP)
                continue
            }
            if (attempt >= QUERY_ATTEMPTS || !failure.isUnavailable()) {
                Timber.w(failure, "TrUAPI chat query for %s has no answer", productId.value)
                return Result.failure(failure.toQueryException())
            }
            attempt++
            delay(QUERY_RETRY_DELAY)
        }
    }

    private fun Throwable.isUnavailable() = this is ChatQueryException.Failed || this is ProductRuntimeException

    // A message in flight holds the worker for a short while, so the product can answer without an open room.
    private suspend fun <T> withBriefHold(block: suspend () -> Result<T>): Result<T> {
        val held = runCatching { holds.hold() }.isSuccess
        try {
            return block()
        } finally {
            if (held) {
                scope.launch {
                    try {
                        delay(BRIEF_HOLD)
                    } finally {
                        holds.drop()
                    }
                }
            }
        }
    }

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> {
        if (roomId == null) return roomlessFailure("message")
        val outcome = withBriefHold { runCatching {
            val execution = awaitRunningExecution()
            execution.publishChatAction(
                HostChatActionSubscribeItem(
                    roomId = roomId.value,
                    peer = NATIVE_PEER,
                    payload = ChatActionPayload.MessagePosted(ChatMessageContent.Text(text)),
                ),
            )
        } }
        outcome.exceptionOrNull()?.let { if (it is CancellationException) throw it }
        return outcome.logFailure("truapi.chat.action for ${productId.value}")
    }

    override suspend fun publishRoomEvent(roomId: ProductChatIdParameter, event: ProductRoomEvent): Result<Unit> {
        val chatEvent = event.toChatEvent() ?: return Result.success(Unit)
        return withBriefHold { publishNow(roomId, chatEvent) }
    }

    private suspend fun publishNow(roomId: ProductChatIdParameter, event: ChatEvent): Result<Unit> {
        val outcome = runCatching { awaitRunningExecution().publishChatEvent(roomId.value, NATIVE_PEER, event) }
        outcome.exceptionOrNull()?.let { if (it is CancellationException) throw it }
        return outcome.logFailure("truapi.chat.event for ${productId.value}")
    }

    override fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> {
        if (roomId == null) return flowOf(roomlessFailure("render"))
        val context = RenderContext.ChatMessage(
            roomId = roomId.value,
            messageId = productMessageId(roomId, messageId),
            messageType = messageType,
        )
        return render(context, messageData.value)
    }

    // The footer render request has an empty payload (02-DESIGN §13 item 14).
    override fun renderFooter(roomId: ProductChatIdParameter): Flow<Result<JsWidget>> =
        render(RenderContext.ChatFooter(roomId.value), ByteArray(0))

    override fun dispatchFooterEvent(roomId: ProductChatIdParameter, actionId: String, eventType: JsUiEvent.Type) {
        val execution = workers.currentExecution(productId)
        if (execution == null) {
            Timber.w("TrUAPI chat footer event for %s/%s dropped: no running execution", productId.value, roomId.value)
            return
        }
        runCatching {
            execution.publishRendererAction(
                HostRendererActionSubscribeItem(RenderContext.ChatFooter(roomId.value), actionId, eventType.toActionPayload()),
            )
        }.logFailure("truapi.renderer.footer action '$actionId' for ${productId.value}")
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun render(context: RenderContext, payload: ByteArray): Flow<Result<JsWidget>> {
        return workers.executionState(productId)
            .distinctUntilChanged()
            .flatMapLatest { state ->
                when (state) {
                    // Reported, not thrown: a later execution must still be able to draw.
                    is WorkerExecutionState.Failed ->
                        flowOf(Result.failure(ExecutionUnavailableException(state.reason)))
                    null -> emptyFlow()
                    is WorkerExecutionState.Running -> state.execution.chatRender(context, payload)
                }
            }
            .conflate()
    }

    override fun dispatchEvent(event: JsUiEvent) {
        val roomId = event.roomId
        if (roomId == null) {
            Timber.w("TrUAPI chat dispatchEvent for %s/%s dropped: no room id", productId.value, event.messageId)
            return
        }
        val execution = workers.currentExecution(productId)
        if (execution == null) {
            Timber.w("TrUAPI chat dispatchEvent for %s/%s dropped: no running execution", productId.value, event.messageId)
            return
        }
        val context = RenderContext.ChatMessage(
            roomId = roomId.value,
            messageId = productMessageId(roomId, event.messageId),
            messageType = event.messageType,
        )
        runCatching {
            execution.publishRendererAction(
                HostRendererActionSubscribeItem(context, event.actionId, event.eventType.toActionPayload()),
            )
        }.logFailure("truapi.renderer.action '${event.actionId}' for ${productId.value}")
    }

    // A message that the product pushed has a store id that starts with its room; the product knows it without.
    private fun productMessageId(roomId: ProductChatIdParameter, storedId: ChatMessageId): String =
        ProductMessageIds.product(storedId, roomId.toChatId(rendererId))

    private fun ProductRoomEvent.toChatEvent(): ChatEvent? = when (this) {
        is ProductRoomEvent.MessagePosted -> content.toCoreBody()?.let { ChatEvent.MessagePosted(messageId, it, replyTo) }
        is ProductRoomEvent.UserEdited -> content.toCoreBody()?.let { ChatEvent.UserEdited(messageId, it) }
        is ProductRoomEvent.UserDeleted -> ChatEvent.UserDeleted(messageId)
        is ProductRoomEvent.UserReacted -> ChatEvent.UserReacted(messageId, emoji, placed)
        is ProductRoomEvent.Seen -> ChatEvent.Seen(messageId)
        is ProductRoomEvent.ActionTriggered -> ChatEvent.ActionTriggered(ActionTrigger(messageId, actionId, payload = null))
        is ProductRoomEvent.Command -> ChatEvent.Command(ChatCommand(command = name, payload = arguments))
        is ProductRoomEvent.MessageActionTriggered -> ChatEvent.MessageActionTriggered(messageId, actionId)
        is ProductRoomEvent.ParticipantActionTriggered -> ChatEvent.ParticipantActionTriggered(participantId, actionId)
        is ProductRoomEvent.ToolbarActionTriggered -> ChatEvent.ToolbarActionTriggered(actionId)
        is ProductRoomEvent.SuggestionSelected -> ChatEvent.SuggestionSelected(suggestionId)
        ProductRoomEvent.RoomOpened -> ChatEvent.RoomOpened
        ProductRoomEvent.RoomClosed -> ChatEvent.RoomClosed
    }

    private fun <T> roomlessFailure(what: String): Result<T> {
        val reason = "TrUAPI chat $what for ${productId.value} carries no room id"
        Timber.w(reason)
        return Result.failure(IllegalArgumentException(reason))
    }

    private suspend fun runningExecution(): TrUAPIProductExecution =
        when (val state = workers.executionState(productId).filterNotNull().first()) {
            is WorkerExecutionState.Running -> state.execution
            is WorkerExecutionState.Failed -> throw ExecutionUnavailableException(state.reason)
        }

    private suspend fun awaitRunningExecution(): TrUAPIProductExecution =
        withTimeoutOrNull(TrUAPIWorkerSupervisor.EXECUTION_WAIT_TIMEOUT) { runningExecution() }
            ?: throw IllegalStateException(
                "TrUAPI worker for ${productId.value} did not report a running execution in time",
            )

    // Reopened under flatMapLatest, not above it: a stop or a replacement execution then cancels
    // the backoff wait, and a failure cannot overtake the node drawn just before it by cancelling
    // the scope that drains flatMapLatest's channel.
    private fun TrUAPIProductExecution.chatRender(context: RenderContext, payload: ByteArray): Flow<Result<JsWidget>> =
        flow {
            var drew = false
            render(ProductRendererRenderRequest(context, payload))
                .retryWhileConnecting()
                .collect { node ->
                    drew = true
                    emit(Result.success(node.toJsWidget()))
                }
            // A stream closed before its first tree would otherwise leave the cell loading for the
            // execution's whole life; a card keeps its last face, a cell has none.
            if (!drew) throw StreamDrewNothing()
        }
            // The execution went away, not the render: end quietly and let the replacement draw.
            .catch { cause -> if (cause !is CancellationException) throw cause }
            .reopenAfterFailure("TrUAPI chat render for ${productId.value}/${context.label()}")

    private fun RenderContext.label(): String = when (this) {
        is RenderContext.ChatMessage -> messageId
        is RenderContext.ChatFooter -> "footer:$roomId"
        else -> toString()
    }

    private class StreamDrewNothing : Exception("the render stream ended without drawing")

    private fun JsUiEvent.Type.toActionPayload(): ByteArray = when (this) {
        JsUiEvent.Type.ButtonClick -> ByteArray(0)
        is JsUiEvent.Type.InputFieldValueChange -> newValue.toByteArray(Charsets.UTF_8)
    }

    private class ExecutionUnavailableException(cause: Throwable) : Exception(cause.message, cause)

    private companion object {
        const val NATIVE_PEER = "native"
        const val QUERY_ATTEMPTS = 2
        val QUERY_RETRY_DELAY = 1.seconds
        const val CONNECT_WAITS = 40
        val CONNECT_WAIT_STEP = 250.milliseconds
        val BRIEF_HOLD = 30.seconds
    }
}
