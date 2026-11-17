package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

internal val productRoom = ChatId.forExtensionRoom("ProductBot_weather", "lobby")

internal fun productMessage(
    index: Int,
    id: String = "m$index",
    participant: String = "product",
    timestamp: Long = index * 1_000L,
    status: ProductMessageStatusModel = ProductMessageStatusModel.SENT,
) = ProductRoomMessage(
    messageId = id,
    participantId = participant,
    timestamp = timestamp,
    content = ChatMessage.Content.Text("message $index"),
    status = status,
    replyTo = null,
    editedAt = null,
    reactions = emptyList(),
    request = false,
)
