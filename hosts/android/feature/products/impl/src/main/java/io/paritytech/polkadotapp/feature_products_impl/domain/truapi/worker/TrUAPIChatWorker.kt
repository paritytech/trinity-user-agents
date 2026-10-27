package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.ChatQueryException
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toModel
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toQueryException
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
import uniffi.truapi.ChatActionPayload
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductChatMessagesQueryRequest
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.ProductRuntimeException
import uniffi.truapi.RenderContext
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class TrUAPIChatWorker(
    private val productId: ProductId,
    private val runtime: TrUAPIHostRuntime,
    private val workers: TrUAPIWorkerSupervisor,
    private val chatMessaging: ProductChatMessaging,
    private val scope: CoroutineScope,
) : ProductWorker {
    // Each open room screen holds the worker as well, so a room keeps its product while the user reads it.
    private val holds = WorkerHolds(
        acquire = { runtime.acquireWorker(productId.value) },
        release = ::releaseWorker,
    )

    init {
        val workerJob = scope.coroutineContext.job
        // A product keeps its worker for the life of its chat: a bot posts with no room open.
        runtime.acquireWorker(productId.value)
        workerJob.invokeOnCompletion { releaseWorker() }
        // When the scope ends, the holds release each room hold and refuse each new hold.
        workerJob.invokeOnCompletion { holds.close() }
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

    private fun releaseWorker() {
        runCatching { runtime.releaseWorker(productId.value) }
            .onFailure { Timber.w(it, "TrUAPI releaseWorker failed for %s", productId.value) }
    }

    override suspend fun openRoom(roomId: ProductChatIdParameter): Result<Unit> {
        runCatching { holds.openRoom(roomId.value) }
            .onFailure { return Result.failure<Unit>(it).logFailure("truapi.chat.open for ${productId.value}") }
        workers.restartIfFailed(productId)
        return Result.success(Unit)
    }

    override suspend fun closeRoom(roomId: ProductChatIdParameter) {
        if (!holds.isOpen(roomId.value)) return
        holds.closeRoom(roomId.value)
    }

    override suspend fun messagesPage(
        roomId: ProductChatIdParameter,
        anchor: ChatPageAnchorModel,
        pageSize: Int,
    ): Result<ProductMessagesPage> = query { execution ->
        execution.chatMessagesQuery(ProductChatMessagesQueryRequest(roomId.value, anchor.toCore(), pageSize.toUInt()))
            .toModel()
    }

    // The core holds the worker for each open query. A worker that is not running yet starts first, and an
    // unavailable worker is asked once more after a second. The supervisor reports a restarted worker as
    // running before its client connects to the core, so a query waits for that connection for a limited time.
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

    override suspend fun onUserMessage(roomId: ProductChatIdParameter?, text: String): Result<Unit> {
        if (roomId == null) return roomlessFailure("message")
        val outcome = runCatching {
            val execution = awaitRunningExecution()
            execution.publishChatAction(
                HostChatActionSubscribeItem(
                    roomId = roomId.value,
                    peer = NATIVE_PEER,
                    payload = ChatActionPayload.MessagePosted(ChatMessageContent.Text(text)),
                ),
            )
        }
        outcome.exceptionOrNull()?.let { if (it is CancellationException) throw it }
        return outcome.logFailure("truapi.chat.action for ${productId.value}")
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> {
        if (roomId == null) return flowOf(roomlessFailure("render"))
        val context = RenderContext.ChatMessage(roomId = roomId.value, messageId = messageId, messageType = messageType)
        return workers.executionState(productId)
            .distinctUntilChanged()
            .flatMapLatest { state ->
                when (state) {
                    // Reported, not thrown: a later execution must still be able to draw.
                    is WorkerExecutionState.Failed ->
                        flowOf(Result.failure(ExecutionUnavailableException(state.reason)))
                    null -> emptyFlow()
                    is WorkerExecutionState.Running -> state.execution.chatRender(context, messageData.value)
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
        val context = RenderContext.ChatMessage(roomId = roomId.value, messageId = event.messageId, messageType = event.messageType)
        runCatching {
            execution.publishRendererAction(
                HostRendererActionSubscribeItem(context, event.actionId, event.eventType.toActionPayload()),
            )
        }.logFailure("truapi.renderer.action '${event.actionId}' for ${productId.value}")
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
    private fun TrUAPIProductExecution.chatRender(context: RenderContext.ChatMessage, payload: ByteArray): Flow<Result<JsWidget>> =
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
            .reopenAfterFailure("TrUAPI chat render for ${productId.value}/${context.messageId}")

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
    }
}
