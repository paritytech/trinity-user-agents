package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.extractProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.reopenAfterStreamEnd
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.renderWidgets
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.RenderContext
import java.util.concurrent.ConcurrentHashMap

/**
 * The product's core worker as the chat surface drives it: a custom message is drawn by rendering
 * its chat context on the worker's execution, and a press in that body goes back as a renderer
 * action on the same context. Typed replies have no path to the worker here.
 */
class TrUAPIChatProductWorker(
    private val productId: ProductId,
    private val extensionId: ChatExtensionId,
    private val workers: TrUAPIWorkerSupervisor,
) : ProductWorker {
    // An action names only its message, while the core needs the context the body was drawn in.
    private val renderedContexts = ConcurrentHashMap<ChatMessageId, RenderContext.ChatMessage>()

    override suspend fun onUserMessage(text: String): Result<Unit> =
        Result.failure(UnsupportedOperationException("typed messages are not delivered to a core worker"))

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderMessage(
        chatId: ChatId,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> {
        val room = chatId.extractProductChatIdParameter(extensionId).getOrElse { return flowOf(Result.failure(it)) }
        val context = RenderContext.ChatMessage(roomId = room.value, messageId = messageId, messageType = messageType)
        renderedContexts[messageId] = context

        return workers.execution(productId)
            .filterNotNull()
            .flatMapLatest { execution -> execution.renderWidgets(context, messageData.value) }
            .reopenAfterStreamEnd("Chat message $messageId of ${productId.value}")
            .map { Result.success(it) }
    }

    override fun dispatchEvent(event: JsUiEvent) {
        val context = renderedContexts[event.messageId] ?: return
        val execution = workers.currentExecution(productId) ?: return
        val payload = when (val type = event.eventType) {
            JsUiEvent.Type.ButtonClick -> ByteArray(0)
            is JsUiEvent.Type.InputFieldValueChange -> type.newValue.encodeToByteArray()
        }

        runCatching { execution.publishRendererAction(HostRendererActionSubscribeItem(context, event.actionId, payload)) }
            .logFailure("truapi.renderer.action '${event.actionId}' for chat message ${event.messageId}")
    }
}
