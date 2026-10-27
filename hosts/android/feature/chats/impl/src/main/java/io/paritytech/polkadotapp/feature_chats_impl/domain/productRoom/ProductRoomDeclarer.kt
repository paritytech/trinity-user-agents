package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import javax.inject.Inject

/** Applies what an extension declares for one of its rooms (Chat Extension v2). */
class ProductRoomDeclarer @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
) {
    suspend fun declarations(extensionId: ChatExtensionId, chatId: ChatId): ChatRoomDeclarations? =
        if (chatId.isExtensionChat(extensionId)) declarationsRepository.get(chatId) else null

    suspend fun setConfig(extensionId: ChatExtensionId, chatId: ChatId, config: ChatRoomConfigModel) = declare(extensionId, chatId) {
        val current = declarationsRepository.get(chatId)?.effectiveConfig ?: ChatRoomConfigModel.HOST_DEFAULT
        // The messages of one store mode mean nothing in the other one, so they go with the change.
        if (current.productSource != config.productSource) chatMessageRepository.deleteAllChatMessages(chatId)
        declarationsWriter.setConfig(chatId, config)
    }

    suspend fun setMetadata(extensionId: ChatExtensionId, chatId: ChatId, name: String, metadata: ChatRoomMetadataModel) =
        declare(extensionId, chatId) {
            chatRoomRepository.renameRoom(chatId, name)
            declarationsWriter.setMetadata(chatId, metadata)
        }

    suspend fun setPreview(extensionId: ChatExtensionId, chatId: ChatId, preview: ChatRoomPreviewModel) =
        declare(extensionId, chatId) { declarationsWriter.setPreview(chatId, preview) }

    private suspend fun declare(extensionId: ChatExtensionId, chatId: ChatId, write: suspend () -> Unit): Result<Unit> =
        runCancellableCatching {
            if (!chatId.isExtensionChat(extensionId) || !chatRoomRepository.roomExists(chatId)) throw UnknownChatRoomException()
            write()
        }
}
