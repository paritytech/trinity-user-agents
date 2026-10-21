package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomUserStateWriter
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import javax.inject.Inject

/** Sends the events of the Chat Extension v2 additions (mute, archive, calls, branches) to the extension of a room. */
interface ChatRoomEventPublisher {
    suspend fun publish(chatId: ChatId, event: ChatRoomEventModel)
}

class RealChatRoomEventPublisher @Inject constructor(
    private val chatExtensionRegistry: ChatExtensionRegistry,
) : ChatRoomEventPublisher {
    override suspend fun publish(chatId: ChatId, event: ChatRoomEventModel) {
        chatExtensionRegistry.getExtensionForChat(chatId)?.onRoomEvent(chatId, event)
    }
}

/** Sets the mute and the archive of a product room, and tells its product (07-LATER §6). */
class ChatRoomUserStates @Inject constructor(
    private val writer: ChatRoomUserStateWriter,
    private val publisher: ChatRoomEventPublisher,
) {
    suspend fun set(chatId: ChatId, state: ChatRoomUserStateModel): Result<Unit> = runCancellableCatching {
        require(chatId.isExtensionChat()) { "Only a product room has a mute and an archive state" }
        writer.set(chatId, state)
        publisher.publish(chatId, ChatRoomEventModel.UserStateChanged(state))
    }
}
