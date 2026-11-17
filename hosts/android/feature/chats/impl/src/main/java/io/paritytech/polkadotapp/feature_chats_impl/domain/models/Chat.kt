package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel

class Chat(
    val id: ChatId,
    val display: ChatDisplay,
    val preview: ChatPreview,
    val timestamp: Timestamp,
    val unreadBadge: ChatSummaryBadge,
    val hasUnseenReaction: Boolean,
    /** The line that the row shows in place of its preview line. */
    val activity: ChatRoomActivityModel?,
    /**
     * @see [ChatSummary.customPreviewRenderer] for explanation why is it here
     */
    val customPreviewRenderer: CustomChatPreviewRenderer<*>?
)
