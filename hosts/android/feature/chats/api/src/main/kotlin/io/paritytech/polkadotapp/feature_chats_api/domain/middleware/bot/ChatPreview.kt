package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import kotlinx.coroutines.flow.Flow

interface CustomChatPreviewDelegate<T> {
    val provider: CustomChatPreviewDataProvider<T>

    val renderer: CustomChatPreviewRenderer<T>
}

interface CustomChatPreviewDataProvider<T> {
    context(scope: ComputationalScope)
    fun provide(): Flow<ChatPreview.Custom<T>?>
}

interface CustomChatPreviewRenderer<T> {
    @Composable
    fun formatChatPreview(data: T): Result<String>
}

typealias CustomChatPreviewDelegatesById = Map<ChatId, CustomChatPreviewDelegate<*>>

@Suppress("UNCHECKED_CAST")
fun CustomChatPreviewRenderer<*>.asAnyRenderer(): CustomChatPreviewRenderer<Any?> {
    return this as CustomChatPreviewRenderer<Any?>
}

sealed interface ChatPreview {
    val order: Order

    /**
     * Note: [T] should properly implement equals&hashCode (e.g. via data class) so that ChatEngine can
     * deduplicate emissions from subscriptions
     */
    data class Custom<T>(
        override val order: Order,
        val data: CustomPreviewData<T>,
        val badgeStyle: BadgeStyle,
    ) : ChatPreview {
        enum class BadgeStyle {
            UNREAD, NOTIFICATION, NONE
        }
    }

    data class Message(
        val message: ChatMessage,
        override val order: Order = Order.ByTimestamp,
    ) : ChatPreview

    data object EmptyChat : ChatPreview {
        override val order = Order.ByTimestamp
    }
}

/**
 * Content of a [ChatPreview.Custom]. A provider can either supply a custom payload to be
 * rendered by its [CustomChatPreviewRenderer], or defer to the last message of the chat
 * (so the row reads like a normal [ChatPreview.Message] but the custom [Order]/badge still apply).
 */
sealed interface CustomPreviewData<out T> {
    /** Render the preview using the chat's last message instead of a custom payload. */
    data object FromMessage : CustomPreviewData<Nothing>

    /** Render the preview via [CustomChatPreviewRenderer], passing [data] as the payload. */
    data class RendererPayload<T>(val data: T) : CustomPreviewData<T>
}
