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
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductSearchPage
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
import uniffi.truapi.ChatWakeOutcome
import uniffi.truapi.ChatWakeReason
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductChatMessageActionsQueryRequest
import uniffi.truapi.ProductChatMessagesQueryRequest
import uniffi.truapi.ProductChatParticipantActionsQueryRequest
import uniffi.truapi.ProductChatParticipantsQueryRequest
import uniffi.truapi.ProductChatReactionsQueryRequest
import uniffi.truapi.ProductChatSearchQueryRequest
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

    // The worker runs while a room screen is open, or for a wake of the core (07-LATER §10.2); never for the life
    // of the installed integration (D10).
    private val holds = WorkerHolds(
        acquire = { runtime.acquireWorker(productId.value) },
        release = {
            runCatching { runtime.releaseWorker(productId.value) }
                .onFailure { Timber.w(it, "TrUAPI releaseWorker failed for %s", productId.value) }
        },
    )
    private val roomAnnouncements = ConcurrentHashMap<String, Job>()

    init {
        // When the scope ends, the holds release each room hold and refuse each new hold. A wake of the core keeps
        // the worker without a room (07-LATER §10.2).
        scope.coroutineContext.job.invokeOnCompletion { holds.close() }
        scope.launch {
            var ran = false
            workers.executionState(productId)
                .map { (it as? WorkerExecutionState.Running)?.execution }
                .distinctUntilChanged()
                .collectLatest { execution ->
                    if (execution == null) {
                        // A stopped worker cannot answer a ringing call, so the rooms stop to ring (07-LATER §11.2).
                        if (ran) chatMessaging.clearRingingCalls().logFailure("truapi.chat.calls for ${productId.value}")
                        return@collectLatest
                    }
                    ran = true
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

    // W2: a product without a room has no screen to start it. The wake lasts until the worker runs, and 30 s more.
    override fun holdBriefly() {
        scope.launch {
            withWake(ChatWakeReason.ProductStart) {
                withTimeoutOrNull(TrUAPIWorkerSupervisor.EXECUTION_WAIT_TIMEOUT) { runningExecution() }
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
        // W1: a user action is a message in flight too, so the product can answer after the room closes.
        return withBriefHold(roomId) {
            query { execution ->
                execution.chatUserAction(ProductChatUserActionRequest(roomId.value, coreAction)).toModel(rendererId)
            }
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

    override suspend fun searchPage(roomId: ProductChatIdParameter, text: String, cursor: String?, pageSize: Int): Result<ProductSearchPage> =
        query { execution ->
            execution.chatSearchQuery(ProductChatSearchQueryRequest(roomId.value, text, cursor, pageSize.toUInt())).toModel(rendererId)
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

    // W1: a message in flight wakes the worker, so the product can answer when the user closes the room at once.
    // Every message wakes, also with an open room: the room can close before the product answers.
    private suspend fun <T> withBriefHold(roomId: ProductChatIdParameter, block: suspend () -> Result<T>): Result<T> =
        withWake(ChatWakeReason.MessageInFlight(roomId.value), block)

    /**
     * Wakes the worker for [reason] while [block] runs, and for 30 s after it (07-LATER §10.2). The core holds a wake
     * for 30 s and a new wake moves its end, so the wake starts again while a slow worker start continues.
     */
    private suspend fun <T> withWake(reason: ChatWakeReason, block: suspend () -> T): T {
        wake(reason)
        val renewal = scope.launch {
            while (true) {
                delay(WAKE_RENEWAL)
                wake(reason)
            }
        }
        try {
            return block()
        } finally {
            renewal.cancel()
            wake(reason)
        }
    }

    // A wake that the core refuses holds nothing, and the message goes on as before.
    private fun wake(reason: ChatWakeReason) {
        runCatching {
            when (val outcome = runtime.wakeProduct(productId.value, reason)) {
                is ChatWakeOutcome.Held -> Timber.d("TrUAPI %s woken for %s until %d", productId.value, reason.label(), outcome.untilMs.toLong())
                ChatWakeOutcome.BudgetSpent -> Timber.w("TrUAPI %s has no wake left for %s", productId.value, reason.label())
            }
        }.onFailure { Timber.w(it, "TrUAPI %s was not woken for %s", productId.value, reason.label()) }
    }

    private fun ChatWakeReason.label(): String = when (this) {
        ChatWakeReason.ProductStart -> "product-start"
        is ChatWakeReason.MessageInFlight -> "message-in-flight $roomId"
        is ChatWakeReason.Notification -> "notification $roomId $watchId"
    }

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> {
        if (roomId == null) return roomlessFailure("message")
        val outcome = withBriefHold(roomId) { runCatching {
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
        return withBriefHold(roomId) { publishNow(roomId, chatEvent) }
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
        }.onSuccess { wakeAfterAction(roomId) }
            .logFailure("truapi.renderer.footer action '$actionId' for ${productId.value}")
    }

    // W1: the product answers a press after the action goes, maybe after the room closes. One wake of the core holds
    // the worker for 30 s.
    private fun wakeAfterAction(roomId: ProductChatIdParameter) = wake(ChatWakeReason.MessageInFlight(roomId.value))

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
        }.onSuccess { wakeAfterAction(roomId) }
            .logFailure("truapi.renderer.action '${event.actionId}' for ${productId.value}")
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

        /** Below the 30 s of a wake of the core, so that a renewal comes before the end. */
        val WAKE_RENEWAL = 20.seconds
    }
}
