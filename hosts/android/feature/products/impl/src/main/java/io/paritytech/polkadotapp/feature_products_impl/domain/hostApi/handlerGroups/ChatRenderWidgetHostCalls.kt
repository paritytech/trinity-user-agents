package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow

/**
 * Handler group for the `chatRenderWidget` host call.
 *
 * Receives render updates from JS and routes them to per-message flows
 * that [ProductsMessageRenderer] observes.
 */
class ChatRenderWidgetHostCalls : HostCallHandlerGroup {
    private val renderUpdates = mutableMapOf<ChatMessageId, MutableSharedFlow<String>>()

    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<ChatRenderWidgetParams, Unit>("chatRenderWidget") { params ->
            renderUpdatesForMessage(params.messageId).tryEmit(params.scaleHex)
            Result.success(Unit)
        }
    }

    fun renderUpdatesForMessage(messageId: ChatMessageId): MutableSharedFlow<String> {
        return renderUpdates.getOrPut(messageId) {
            MutableSharedFlow(replay = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)
        }
    }

    fun removeMessage(messageId: ChatMessageId) {
        renderUpdates.remove(messageId)
    }
}

private data class ChatRenderWidgetParams(val messageId: String, val scaleHex: String)
