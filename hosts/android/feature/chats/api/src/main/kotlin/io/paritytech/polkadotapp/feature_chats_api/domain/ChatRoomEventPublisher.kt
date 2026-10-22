package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel

/** Sends the events of the Chat Extension v2 additions, such as the answer to a call, to the extension of a room. */
interface ChatRoomEventPublisher {
    suspend fun publish(chatId: ChatId, event: ChatRoomEventModel)
}
