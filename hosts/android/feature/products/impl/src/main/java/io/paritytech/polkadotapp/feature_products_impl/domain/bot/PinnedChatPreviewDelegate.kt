package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDataProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDelegate
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf

/**
 * Pins a chat above the rest because the host placed it, so it does not compete on recency: on a
 * fresh install every other chat gets a message later than the placed one. The row itself reads
 * like any other, from its last message, so the renderer is never asked to draw.
 */
internal object PinnedChatPreviewDelegate :
    CustomChatPreviewDelegate<Nothing>,
    CustomChatPreviewDataProvider<Nothing>,
    CustomChatPreviewRenderer<Nothing> {
    override val provider: CustomChatPreviewDataProvider<Nothing> get() = this

    override val renderer: CustomChatPreviewRenderer<Nothing> get() = this

    context(scope: ComputationalScope)
    override fun provide(): Flow<ChatPreview.Custom<Nothing>?> = flowOf(
        ChatPreview.Custom(Order.PinToTop, CustomPreviewData.FromMessage, ChatPreview.Custom.BadgeStyle.UNREAD)
    )

    @Composable
    override fun formatChatPreview(data: Nothing): Result<String> =
        error("a FromMessage preview is drawn from the chat's last message")
}
