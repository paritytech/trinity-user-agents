package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isUser
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot

/**
 * A message of the window as a query to the product carries it (02-DESIGN §13 item 14). The ids stay the ids of the
 * room screen, and the extension of the room changes them into product ids. The latest edit gives the body.
 */
internal fun ChatFeedSnapshot.productQueryMessage(message: ChatMessage): ProductRoomMessage {
    val revision: MessageRevision? = revisions[message.id]
    val isFromUser = message.origin.isUser()

    return ProductRoomMessage(
        messageId = message.id,
        participantId = if (isFromUser) ProductParticipant.USER else message.originParticipantId ?: ProductParticipant.PRODUCT,
        timestamp = message.timestamp,
        content = revision?.content ?: message.content,
        status = if (isFromUser) productStatuses[message.id] ?: message.status.toProductStatus() else ProductMessageStatusModel.SENT,
        replyTo = message.replyToMessageId,
        editedAt = revision?.timestamp,
        reactions = reactions[message.id].orEmpty().toSummaries(),
    )
}

private fun ChatMessage.Status.toProductStatus(): ProductMessageStatusModel = when (this) {
    ChatMessage.Status.NEW,
    ChatMessage.Status.PROCESSING -> ProductMessageStatusModel.PREPARING
    ChatMessage.Status.IS_SENT -> ProductMessageStatusModel.SENT
    ChatMessage.Status.IS_READ -> ProductMessageStatusModel.READ
    ChatMessage.Status.DELIVERY_FAILED -> ProductMessageStatusModel.FAILED
}

private fun List<ChatMessageReaction>.toSummaries(): List<ProductReactionSummaryModel> =
    groupBy { it.content.emoji }.map { (emoji, reactions) ->
        ProductReactionSummaryModel(emoji = emoji, count = reactions.size, reactedByUser = reactions.any { it.origin.isUser() })
    }
