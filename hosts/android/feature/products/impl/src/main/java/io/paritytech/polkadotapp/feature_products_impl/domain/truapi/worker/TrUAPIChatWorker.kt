package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.MaterialsResolution
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.holdsMaterials
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
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
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.ChatActionPayload
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductId
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode

class TrUAPIChatWorker(
    private val productId: ProductId,
    private val runtime: TrUAPIHostRuntime,
    private val workers: TrUAPIWorkerSupervisor,
    private val chatMessaging: ProductChatMessaging,
    private val scope: CoroutineScope,
) : ProductWorker {
    private val materials = MutableStateFlow<Map<RenderContext, MaterialsResolution>>(emptyMap())

    init {
        val workerJob = scope.coroutineContext.job
        runtime.acquireWorker(productId)
        workerJob.invokeOnCompletion {
            runCatching { runtime.releaseWorker(productId) }
                .onFailure { Timber.w(it, "TrUAPI releaseWorker failed for %s", productId.value) }
        }
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

    override fun renderMessage(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> {
        if (roomId == null) return flowOf(roomlessFailure("render"))
        return render(
            RenderContext.ChatMessage(roomId = roomId.value, messageId = messageId, messageType = messageType),
            messageData.value,
            "${productId.value}/$messageId",
        )
    }

    override fun messageMaterials(
        roomId: ProductChatIdParameter?,
        messageId: ChatMessageId,
        messageType: String,
    ): Flow<MaterialsResolution?> {
        if (roomId == null) return flowOf(null)
        val context = RenderContext.ChatMessage(roomId = roomId.value, messageId = messageId, messageType = messageType)
        return materials.map { it[context] }.distinctUntilChanged()
    }

    /** Draws the card the product docked in the footer of [roomId]. */
    override fun renderRoomFooter(roomId: ProductChatIdParameter, messageType: String, payload: DataByteArray): Flow<Result<JsWidget>> =
        render(
            RenderContext.ChatRoomFooter(roomId = roomId.value, messageType = messageType),
            payload.value,
            "${productId.value} footer of ${roomId.value}",
        )

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun render(context: RenderContext, payload: ByteArray, label: String): Flow<Result<JsWidget>> =
        workers.executionState(productId)
            .distinctUntilChanged()
            .flatMapLatest { state ->
                when (state) {
                    // Reported, not thrown: a later execution must still be able to draw.
                    is WorkerExecutionState.Failed ->
                        flowOf(Result.failure(ExecutionUnavailableException(state.reason)))
                    null -> emptyFlow()
                    is WorkerExecutionState.Running -> state.execution.chatRender(context, payload, label)
                }
            }
            .conflate()

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
        execution.publishRendererAction(context, event.actionId, event.eventType)
    }

    /** A press inside the card the product docked in the footer of [roomId]. */
    override fun dispatchRoomFooterEvent(roomId: ProductChatIdParameter, messageType: String, actionId: String, eventType: JsUiEvent.Type) {
        val execution = workers.currentExecution(productId)
        if (execution == null) {
            Timber.w("TrUAPI chat footer event for %s/%s dropped: no running execution", productId.value, roomId.value)
            return
        }
        execution.publishRendererAction(RenderContext.ChatRoomFooter(roomId = roomId.value, messageType = messageType), actionId, eventType)
    }

    private fun TrUAPIProductExecution.publishRendererAction(context: RenderContext, actionId: String, eventType: JsUiEvent.Type) {
        runCatching {
            publishRendererAction(HostRendererActionSubscribeItem(context, actionId, eventType.toActionPayload()))
        }.logFailure("truapi.renderer.action '$actionId' for ${productId.value}")
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
    private fun TrUAPIProductExecution.chatRender(context: RenderContext, payload: ByteArray, label: String): Flow<Result<JsWidget>> =
        flow {
            var drew = false
            var resolution: MaterialsResolution? = null
            try {
                render(ProductRendererRenderRequest(context, payload))
                    .retryWhileConnecting()
                    .collect { node ->
                        drew = true
                        val replaced = resolution
                        resolution = resolveChatMaterials(node, context)
                        replaceMaterials(context, replaced, resolution)
                        replaced?.close()
                        emit(Result.success(node.toJsWidget()))
                    }
            } finally {
                replaceMaterials(context, resolution, null)
                resolution?.close()
            }
            // A stream closed before its first tree would otherwise leave the cell loading for the
            // execution's whole life; a card keeps its last face, a cell has none.
            if (!drew) throw StreamDrewNothing()
        }
            // The execution went away, not the render: end quietly and let the replacement draw.
            .catch { cause -> if (cause !is CancellationException) throw cause }
            .reopenAfterFailure("TrUAPI chat render for $label")

    // The footer card is drawn by a renderer with no materials drawer, so only messages resolve.
    private fun TrUAPIProductExecution.resolveChatMaterials(node: RendererNode, context: RenderContext): MaterialsResolution? =
        if (context is RenderContext.ChatMessage && node.holdsMaterials()) {
            runCatching { resolveMaterials(node, context, floatFilterable = false) }
                .logFailure("truapi.materials.resolve for ${productId.value}")
                .getOrNull()
        } else {
            null
        }

    // Another collector of the same message may have published since, and its entry stays.
    private fun replaceMaterials(context: RenderContext, replaced: MaterialsResolution?, resolution: MaterialsResolution?) {
        materials.update { current ->
            when {
                resolution != null -> current + (context to resolution)
                current[context] === replaced -> current - context
                else -> current
            }
        }
    }

    private class StreamDrewNothing : Exception("the render stream ended without drawing")

    private fun JsUiEvent.Type.toActionPayload(): ByteArray = when (this) {
        JsUiEvent.Type.ButtonClick -> ByteArray(0)
        is JsUiEvent.Type.InputFieldValueChange -> newValue.toByteArray(Charsets.UTF_8)
    }

    private class ExecutionUnavailableException(cause: Throwable) : Exception(cause.message, cause)

    private companion object {
        const val NATIVE_PEER = "native"
    }
}
