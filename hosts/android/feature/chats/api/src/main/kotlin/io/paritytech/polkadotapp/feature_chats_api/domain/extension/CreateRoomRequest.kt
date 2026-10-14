package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

class CreateRoomRequest(
    val chatId: ChatId,
    val name: String?,
    val icon: String?,
)

class CreateRoomResult(
    val chatId: ChatId,
    val status: CreateRoomStatus,
)

enum class CreateRoomStatus {
    New,
    Exists,
}
