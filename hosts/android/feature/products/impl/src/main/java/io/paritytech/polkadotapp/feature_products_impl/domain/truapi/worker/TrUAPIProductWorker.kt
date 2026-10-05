package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import timber.log.Timber
import uniffi.truapi.ChatActionPayload
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import java.util.concurrent.ConcurrentHashMap

class TrUAPIProductWorker(
    private val productId: ProductId,
    private val workers: TrUAPIWorkerSupervisor,
) : ProductWorker {
    private val contexts = ConcurrentHashMap<ChatMessageId, RenderContext>()

    override suspend fun onUserMessage(roomId: String, text: String): Result<Unit> = runCatching {
        workers.execution(productId).filterNotNull().first().publishChatAction(
            HostChatActionSubscribeItem(roomId, "native", ChatActionPayload.MessagePosted(ChatMessageContent.Text(text))),
        )
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderMessage(roomId: String, messageId: ChatMessageId, messageType: String, messageData: DataByteArray): Flow<Result<JsWidget>> {
        val context = RenderContext.ChatMessage(roomId, messageId, messageType)
        contexts[messageId] = context
        return workers.execution(productId).filterNotNull().flatMapLatest { execution ->
            execution.render(ProductRendererRenderRequest(context, messageData.value))
                .map { Result.success(it.toJsWidget()) }
                .catch { emit(Result.failure(it)) }
        }
    }

    override fun dispatchEvent(event: JsUiEvent) {
        val context = contexts[event.messageId] ?: return
        val payload = when (val type = event.eventType) {
            JsUiEvent.Type.ButtonClick -> byteArrayOf()
            is JsUiEvent.Type.InputFieldValueChange -> type.newValue.scaleEncodeBinary()
        }
        runCatching {
            checkNotNull(workers.currentExecution(productId)) { "Product worker is unavailable" }
                .publishRendererAction(HostRendererActionSubscribeItem(context, event.actionId, payload))
        }.onFailure { Timber.e(it, "Product renderer action failed") }
    }
}
