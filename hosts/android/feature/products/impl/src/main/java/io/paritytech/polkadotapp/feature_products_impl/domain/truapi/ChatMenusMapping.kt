package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatBuiltInMessageActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import uniffi.truapi.ChatActionHandler
import uniffi.truapi.ChatBuiltInMessageAction
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageMenu
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatParticipantMenu
import uniffi.truapi.ChatProductAction
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.ChatMessage as CoreChatMessage

// Maps the menus of Chat Extension v2 (02-DESIGN §3.3.8) between the bindings and the chat models of the app.

fun ChatMessageMenu.toModel(): ChatMessageMenuModel = when (this) {
    is ChatMessageMenu.Custom -> ChatMessageMenuModel.Custom(actions.map { it.toModel() })
    is ChatMessageMenu.ExtendBuiltIn -> ChatMessageMenuModel.ExtendBuiltIn(
        actions = actions.map { it.toModel() },
        hidden = hidden.mapTo(mutableSetOf()) { it.toModel() },
    )
}

fun ChatParticipantMenu.toModel(): ChatParticipantMenuModel = when (this) {
    is ChatParticipantMenu.Custom -> ChatParticipantMenuModel.Custom(actions.map { it.toModel() })
    is ChatParticipantMenu.ExtendBuiltIn -> ChatParticipantMenuModel.ExtendBuiltIn(actions.map { it.toModel() })
}

fun ChatProductAction.toModel() = ChatProductActionModel(
    actionId = actionId,
    label = label,
    icon = icon?.toModel(),
    destructive = destructive,
    handler = handler.toModel(),
)

fun ChatActionHandler.toModel(): ChatActionHandlerModel = when (this) {
    is ChatActionHandler.Deeplink -> ChatActionHandlerModel.Deeplink(url)
    ChatActionHandler.Callback -> ChatActionHandlerModel.Callback
}

private fun ChatBuiltInMessageAction.toModel(): ChatBuiltInMessageActionModel = when (this) {
    ChatBuiltInMessageAction.COPY -> ChatBuiltInMessageActionModel.COPY
    ChatBuiltInMessageAction.REPLY -> ChatBuiltInMessageActionModel.REPLY
    ChatBuiltInMessageAction.EDIT -> ChatBuiltInMessageActionModel.EDIT
    ChatBuiltInMessageAction.DELETE -> ChatBuiltInMessageActionModel.DELETE
    ChatBuiltInMessageAction.REACT -> ChatBuiltInMessageActionModel.REACT
    ChatBuiltInMessageAction.RETRY -> ChatBuiltInMessageActionModel.RETRY
}

/**
 * The message as `message_actions_query` carries it (02-DESIGN §13 item 14). A `Custom` body has an empty payload, and
 * media items stay out: the product knows its messages by id, and inline media can be up to 1 MiB each.
 */
fun ProductRoomMessage.toQueryMessage() = CoreChatMessage(
    messageId = messageId,
    origin = ChatMessageOrigin(participantId),
    timestamp = timestamp.toULong(),
    body = content.toQueryBody(),
    status = status.toCore(),
    replyTo = replyTo,
    editedAt = editedAt?.toULong(),
    reactions = reactions.map { ChatReactionSummary(it.emoji, it.count.toUInt(), it.reactedByUser) },
)

private fun ChatMessage.Content.toQueryBody(): ChatMessageBody = when (this) {
    is ChatMessage.Content.Text -> ChatMessageBody.Text(text)
    is ChatMessage.Content.RichText -> ChatMessageBody.RichText(text, emptyList())
    is ChatMessage.Content.ProductBody -> when (val body = body) {
        is ProductMessageBody.RichText -> ChatMessageBody.RichText(body.text, emptyList())
        is ProductMessageBody.Actions -> ChatMessageBody.Actions(body.toCore())
    }
    is ChatMessage.Content.Custom<*> -> (content.getOrNull() as? ProductsMessageContent)
        ?.let { ChatMessageBody.Custom(it.messageType, ByteArray(0), it.alt) }
        ?: ChatMessageBody.Text("")
    else -> ChatMessageBody.Text("")
}

private fun ProductMessageStatusModel.toCore(): ChatMessageStatus = when (this) {
    ProductMessageStatusModel.PREPARING -> ChatMessageStatus.PREPARING
    ProductMessageStatusModel.SENT -> ChatMessageStatus.SENT
    ProductMessageStatusModel.DELIVERED -> ChatMessageStatus.DELIVERED
    ProductMessageStatusModel.READ -> ChatMessageStatus.READ
    ProductMessageStatusModel.FAILED -> ChatMessageStatus.FAILED
}
