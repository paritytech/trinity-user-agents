package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.toChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy

/**
 * The row of a product room. The product of a room that keeps its messages gives the badge, the time and the newest
 * message or reaction, which the row draws as its last message.
 */
internal fun ChatSummary.withRoomPolicy(policy: ChatRoomPolicy?): ChatSummary {
    if (policy == null) return this
    val keepsMessages = policy.messageStore == ChatRoomPolicy.MessageStore.PRODUCT

    val productBadge = when (val badge = policy.listBadge) {
        // The host counts nothing in a room whose product keeps the messages.
        ChatRoomPolicy.ListBadge.HostUnreadCount -> if (keepsMessages) ChatSummaryBadge.None else this.badge
        ChatRoomPolicy.ListBadge.Hidden -> ChatSummaryBadge.None
        is ChatRoomPolicy.ListBadge.Count -> unreadBadge(badge.count)
    }
    val listPreview = policy.listPreview ?: return copy(badge = productBadge, hasUnseenReaction = hasUnseenReaction && !keepsMessages)

    return copy(
        badge = productBadge,
        hasUnseenReaction = listPreview.unseenReactions > 0,
        preview = previewMessage(listPreview, policy.listTimestamp)?.let { ChatPreview.Message(it) } ?: preview,
        timestamp = policy.listTimestamp ?: timestamp,
    )
}

private fun unreadBadge(count: Int): ChatSummaryBadge = if (count > 0) ChatSummaryBadge.Unread(count) else ChatSummaryBadge.None

// A reaction newer than the newest message shows at its own time, which the product gives as the time of the room.
private fun ChatSummary.previewMessage(listPreview: ChatRoomPolicy.ListPreview, listTimestamp: Long?): ChatMessage? {
    val extensionId = chatId.extensionOrNull()?.extensionId ?: return null
    val lastMessage = listPreview.lastMessage?.toChatMessage(chatId, extensionId, isUnread = false)
    val reaction = listPreview.lastReaction ?: return lastMessage

    val reactedId = reaction.message?.messageId ?: lastMessage?.id.orEmpty()
    return ChatMessage(
        id = "$REACTION_PREVIEW_PREFIX$reactedId",
        chatId = chatId,
        timestamp = listTimestamp ?: lastMessage?.timestamp ?: timestamp,
        origin = if (reaction.byUser) ChatMessageOrigin.User else ChatMessageOrigin.Extension(extensionId),
        // The row says "removed reaction" for the removal of a reaction.
        content = if (reaction.removed) {
            ChatMessage.Content.ReactionRemoved(reactedId, ChatMessageReactionContent(reaction.emoji))
        } else {
            ChatMessage.Content.Reacted(reactedId, ChatMessageReactionContent(reaction.emoji))
        },
        status = ChatMessage.Status.IS_READ,
    )
}

private const val REACTION_PREVIEW_PREFIX = "reaction:"
