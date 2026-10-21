package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import javax.inject.Inject

/**
 * Applies the Chat Extension v2 additions that an extension sets for its rooms: the features and the call. The
 * host keeps each value also when it does not serve its feature, so a later build applies it.
 */
class ProductRoomAdditions @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
    private val callWriter: ChatRoomCallWriter,
) {
    suspend fun setFeatures(extensionId: ChatExtensionId, chatId: ChatId, features: ChatRoomFeaturesModel): Result<Unit> =
        runCancellableCatching {
            requireOwnRoom(extensionId, chatId)
            declarationsWriter.setFeatures(chatId, features)
        }

    suspend fun setCall(extensionId: ChatExtensionId, chatId: ChatId, call: ChatRoomCallModel?): Result<Unit> =
        runCancellableCatching {
            requireOwnRoom(extensionId, chatId)
            callWriter.set(chatId, call)
        }

    private suspend fun requireOwnRoom(extensionId: ChatExtensionId, chatId: ChatId) {
        if (!chatId.isExtensionChat(extensionId) || !chatRoomRepository.roomExists(chatId)) throw UnknownChatRoomException()
    }
}
