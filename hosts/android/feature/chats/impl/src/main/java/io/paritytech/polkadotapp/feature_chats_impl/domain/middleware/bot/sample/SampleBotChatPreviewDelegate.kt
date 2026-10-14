package io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.bot.sample

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDataProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDelegate
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class SampleBotChatPreviewDelegate @Inject constructor(
    override val renderer: SampleBotChatPreviewRenderer,
    override val provider: SampleBotChatPreviewDataProvider
) : CustomChatPreviewDelegate<String>

class SampleBotChatPreviewDataProvider @Inject constructor() : CustomChatPreviewDataProvider<String> {
    context(scope: ComputationalScope)
    override fun provide(): Flow<ChatPreview.Custom<String>?> {
        return flowOf {
            ChatPreview.Custom(
                order = Order.ByTimestamp,
                data = CustomPreviewData.RendererPayload("Custom preview for Sample Bot"),
                badgeStyle = ChatPreview.Custom.BadgeStyle.NONE,
            )
        }
    }
}

class SampleBotChatPreviewRenderer @Inject constructor() : CustomChatPreviewRenderer<String> {
    @Composable
    override fun formatChatPreview(data: String): Result<String> {
        return runCatching {
            data
        }
    }
}
