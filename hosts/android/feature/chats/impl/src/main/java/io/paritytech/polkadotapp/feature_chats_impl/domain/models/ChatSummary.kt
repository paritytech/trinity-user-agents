package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order

data class ChatSummary(
    val chatId: ChatId,
    val preview: ChatPreview,
    val order: Order,
    val badge: ChatSummaryBadge,
    val timestamp: Timestamp,
    val lastMessageSortOrder: Long?,
    val roomMetadata: RoomMetadata,
    val hasUnseenReaction: Boolean,
    // TODO this is a workaround to efficiently get access to the custom renderer
    // Previously we have fetched renderers map separately but not it requires extra db access (read all rooms)
    // Ideally we should have a separate ChatPreview model for what DataProvider outputs and what ChatListInteractor constructs,
    // so we can return customPreviewRenderer in ChatPreview from ChatListInteractor instead of storing it here
    // Will only be non null when preview is ChatPreview.Custom
    val customPreviewRenderer: CustomChatPreviewRenderer<*>?
)

data class LastMessageSummary(
    val chatId: ChatId,
    val lastMessage: ChatMessage?,
    val lastMessageSortOrder: Long?,
    val unseenCount: Int,
    val hasUnseenReaction: Boolean,
    val chatCreatedAt: Long,
    val roomMetadata: RoomMetadata,
)

internal val chatSummaryOrder: Comparator<ChatSummary> = compareBy<ChatSummary> { it.order }
    .thenByDescending { it.lastMessageSortOrder }
    .thenByDescending { it.timestamp }
