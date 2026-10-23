package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.toChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.toPendingMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPayment
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.toPendingMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.withPaymentStatus
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy

/**
 * The row of a product room. The product of a room that keeps its messages gives the badge, the time and the newest
 * message or reaction, which the row draws as the native chat draws its last message (02-DESIGN §19 items 2, 3).
 */
internal fun ChatSummary.withRoomPolicy(policy: ChatRoomPolicy?): ChatSummary {
    if (policy == null) return this
    val keepsMessages = policy.messageStore == ChatRoomPolicy.MessageStore.PRODUCT

    // The native chat counts each unseen reaction of the contact as an unread message (base ChatRoomDao unseenCount).
    val unseenReactions = policy.listPreview?.unseenReactions?.takeIf { policy.contact != null } ?: 0
    val productBadge = when (val badge = policy.listBadge) {
        // The host counts nothing in a room whose product keeps the messages.
        ChatRoomPolicy.ListBadge.HostUnreadCount -> if (keepsMessages) unreadBadge(unseenReactions) else this.badge
        ChatRoomPolicy.ListBadge.Hidden -> unreadBadge(unseenReactions)
        is ChatRoomPolicy.ListBadge.Count -> unreadBadge(badge.count + unseenReactions)
    }
    val listPreview = policy.listPreview ?: return copy(badge = productBadge, hasUnseenReaction = hasUnseenReaction && !keepsMessages)

    return copy(
        badge = productBadge,
        hasUnseenReaction = listPreview.unseenReactions > 0,
        preview = previewMessage(listPreview, policy.listTimestamp)?.let { ChatPreview.Message(it) } ?: preview,
        timestamp = policy.listTimestamp ?: timestamp,
    )
}

/**
 * The newest message that the user sent to a person chat of the chat product and that waits in the outbox, when it is
 * newer than what the product lists: the native list shows a message it sends at once.
 */
internal fun ChatSummary.withPendingMessage(
    entry: ProductOutboxEntry?,
    policy: ChatRoomPolicy?,
    preview: (ProductOutboxEntry) -> ChatResourceModel,
): ChatSummary {
    if (entry == null || policy?.contact == null) return this
    if (entry.createdAt < (policy.listTimestamp ?: timestamp)) return this
    // The row shows the newest text of the message, with the edit that waits with it.
    val shown = entry.copy(text = entry.editedText ?: entry.text)
    return copy(preview = ChatPreview.Message(shown.toPendingMessage(preview)), timestamp = entry.createdAt)
}

/**
 * The payments of a person chat of the chat product in its row: the newest message reads with the status that the host
 * keeps for its payment, as the native row reads the status of a payment message, and a payment that waits for the
 * product shows at once when it is newer, as the native list shows a message it sends.
 */
internal fun ChatSummary.withPayments(payments: List<ProductChatPayment>, policy: ChatRoomPolicy?): ChatSummary {
    if (payments.isEmpty() || policy?.contact == null) return this

    val pending = payments.filter { !it.incoming && it.isUndelivered }.maxByOrNull { it.timestamp }
    if (pending != null && pending.timestamp >= timestamp) {
        return copy(preview = ChatPreview.Message(pending.toPendingMessage()), timestamp = pending.timestamp)
    }

    val message = (preview as? ChatPreview.Message)?.message ?: return this
    val payment = payments.firstOrNull { it.messageId == message.id } ?: return this
    return copy(preview = ChatPreview.Message(message.withPaymentStatus(payment)))
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
        // The native row says "removed reaction" for the removal of a reaction.
        content = if (reaction.removed) {
            ChatMessage.Content.ReactionRemoved(reactedId, ChatMessageReactionContent(reaction.emoji))
        } else {
            ChatMessage.Content.Reacted(reactedId, ChatMessageReactionContent(reaction.emoji))
        },
        status = ChatMessage.Status.IS_READ,
    )
}

private const val REACTION_PREVIEW_PREFIX = "reaction:"
