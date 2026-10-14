package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.database.model.ChatMessageReactionLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent

fun ChatMessageReactionLocal.toDomain(): ChatMessageReaction {
    return ChatMessageReaction(
        messageId = messageId,
        content = ChatMessageReactionContent(emoji),
        origin = origin.toDomain(),
        timestamp = timestamp
    )
}

fun ChatMessageReaction.toLocal(chatId: ChatId): ChatMessageReactionLocal {
    return ChatMessageReactionLocal(
        messageId = messageId,
        emoji = content.emoji,
        origin = origin.toLocal(),
        chatId = chatId.toLocal(),
        timestamp = timestamp
    )
}
