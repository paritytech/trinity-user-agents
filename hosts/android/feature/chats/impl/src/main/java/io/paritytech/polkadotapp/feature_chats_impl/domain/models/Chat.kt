package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

class Chat(
    val id: ChatId,
    val display: ChatDisplay,
    val preview: ChatPreview,
    val timestamp: Timestamp,
    val unreadBadge: ChatSummaryBadge,
    val hasUnseenReaction: Boolean,
    /**
     * @see [ChatSummary.customPreviewRenderer] for explanation why is it here
     */
    val customPreviewRenderer: CustomChatPreviewRenderer<*>?
)
