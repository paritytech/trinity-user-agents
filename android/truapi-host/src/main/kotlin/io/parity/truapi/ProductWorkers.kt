package io.parity.truapi

import kotlinx.coroutines.flow.Flow
import uniffi.truapi.ChatRoom
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.NativeWorkerExecution
import uniffi.truapi.NativeWorkerExecutionState
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.ProductRuntimeException
import uniffi.truapi.RendererNode

/** Open the execution for this WebView startup attempt. */
fun NativeWorkerAttempt.openExecution(
    bridge: HostBridge,
    chat: ChatHostBridge?,
    game: GameHostBridge?,
): TrUAPIProductExecution = openExecutionWithBridges(bridge, chat, game, ::openExecution)

/** An execution borrowed by Chat or Pocket while the worker is running. */
class TrUAPIWorkerExecution internal constructor(private val inner: NativeWorkerExecution) {
    @Throws(ProductRuntimeException::class)
    fun publishChatAction(action: HostChatActionSubscribeItem) = inner.publishChatAction(action)

    fun notifyChatRoomsChanged(rooms: List<ChatRoom>) = inner.notifyChatRoomsChanged(rooms)

    @Throws(ProductRuntimeException::class)
    fun publishRendererAction(item: HostRendererActionSubscribeItem) = inner.publishRendererAction(item)

    fun render(request: ProductRendererRenderRequest): Flow<RendererNode> =
        renderFlow { observer -> inner.render(request, observer) }
}

/** The latest state of one product's worker, including startup retries. */
sealed interface WorkerExecutionState {
    data object Idle : WorkerExecutionState
    data object Pending : WorkerExecutionState
    data class Running(val execution: TrUAPIWorkerExecution) : WorkerExecutionState
    data class Failed(val reason: Throwable) : WorkerExecutionState
}

internal fun NativeWorkerExecutionState.toWorkerExecutionState(): WorkerExecutionState = when (this) {
    NativeWorkerExecutionState.Idle -> WorkerExecutionState.Idle
    NativeWorkerExecutionState.Pending -> WorkerExecutionState.Pending
    is NativeWorkerExecutionState.Running -> WorkerExecutionState.Running(TrUAPIWorkerExecution(execution))
    is NativeWorkerExecutionState.Failed -> WorkerExecutionState.Failed(IllegalStateException(reason))
}
