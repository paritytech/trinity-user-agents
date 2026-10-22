package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import javax.inject.Inject

class RealChatRoomEventPublisher @Inject constructor(
    private val chatExtensionRegistry: ChatExtensionRegistry,
) : ChatRoomEventPublisher {
    override suspend fun publish(chatId: ChatId, event: ChatRoomEventModel) {
        chatExtensionRegistry.getExtensionForChat(chatId)?.onRoomEvent(chatId, event)
    }
}
