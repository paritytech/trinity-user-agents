package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

internal val ProductRoomMessage.isFromUser: Boolean
    get() = participantId == ProductParticipant.USER

/** The status of a message of the product tells the room screen whether the user saw it. */
internal fun ProductRoomMessage.toChatMessage(chatId: ChatId, extensionId: ChatExtensionId, isUnread: Boolean) = ChatMessage(
    id = messageId,
    chatId = chatId,
    timestamp = timestamp,
    origin = if (isFromUser) ChatMessageOrigin.User else ChatMessageOrigin.Extension(extensionId),
    content = content,
    status = when {
        isFromUser -> status.toChatStatus()
        isUnread -> ChatMessage.Status.NEW
        else -> ChatMessage.Status.IS_READ
    },
    replyToMessageId = replyTo,
    originParticipantId = participantId.takeUnless { isFromUser },
)

/** One reaction for each count of a summary. The reaction of the user has the user as origin. */
internal fun ProductRoomMessage.toReactions(extensionId: ChatExtensionId): List<ChatMessageReaction> = reactions.flatMap { summary ->
    val content = ChatMessageReactionContent(summary.emoji)
    val productCount = if (summary.reactedByUser) summary.count - 1 else summary.count
    val user = if (summary.reactedByUser) listOf(ChatMessageReaction(messageId, content, ChatMessageOrigin.User, timestamp)) else emptyList()
    val product = List(productCount.coerceAtLeast(0)) {
        ChatMessageReaction(messageId, content, ChatMessageOrigin.Extension(extensionId), timestamp)
    }
    user + product
}

internal fun ProductRoomMessage.toRevision(chatId: ChatId): MessageRevision? =
    editedAt?.let { MessageRevision(messageId = messageId, content = content, chatId = chatId, timestamp = it) }

private fun ProductMessageStatusModel.toChatStatus(): ChatMessage.Status = when (this) {
    ProductMessageStatusModel.PREPARING -> ChatMessage.Status.NEW
    ProductMessageStatusModel.SENT,
    ProductMessageStatusModel.DELIVERED -> ChatMessage.Status.IS_SENT
    ProductMessageStatusModel.READ -> ChatMessage.Status.IS_READ
    ProductMessageStatusModel.FAILED -> ChatMessage.Status.DELIVERY_FAILED
}

/** Counts a reaction once for each participant. The reaction of the user sets `reactedByUser`. */
internal fun ProductRoomMessage.adjustReaction(emoji: String, participantId: String, placed: Boolean): ProductRoomMessage {
    val isUser = participantId == ProductParticipant.USER
    val index = reactions.indexOfFirst { it.emoji == emoji }

    if (index == -1) {
        if (!placed) return this
        return copy(reactions = reactions + ProductReactionSummaryModel(emoji = emoji, count = 1, reactedByUser = isUser))
    }

    val summary = reactions[index]
    // The product can repeat the reaction of the user that the window shows already.
    if (isUser && summary.reactedByUser == placed) return this

    val count = if (placed) summary.count + 1 else (summary.count.coerceAtLeast(1) - 1)
    val reactedByUser = if (isUser) placed else summary.reactedByUser
    val changed = reactions.toMutableList()
    if (count == 0) changed.removeAt(index) else changed[index] = summary.copy(count = count, reactedByUser = reactedByUser)

    return copy(reactions = changed)
}
