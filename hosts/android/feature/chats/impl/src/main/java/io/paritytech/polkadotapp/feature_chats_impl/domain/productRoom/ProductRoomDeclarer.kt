package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatMessageMetaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
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
    private val activityWriter: ChatRoomActivityWriter,
    private val metaWriter: ChatMessageMetaWriter,
) {
    suspend fun declarations(extensionId: ChatExtensionId, chatId: ChatId): ChatRoomDeclarations? =
        if (chatId.isExtensionChat(extensionId)) declarationsRepository.get(chatId) else null

    suspend fun setConfig(extensionId: ChatExtensionId, chatId: ChatId, config: ChatRoomConfigModel) = declare(extensionId, chatId) {
        val current = declarationsRepository.get(chatId)?.effectiveConfig ?: ChatRoomConfigModel.HOST_DEFAULT
        // The same configuration is written too, because each write starts a new generation. An open room then asks
        // the product again for its participants and its menus, as on iOS (02-DESIGN §8).
        // The messages of one store mode mean nothing in the other one, so they go with the change.
        if (current.productSource != config.productSource) {
            chatMessageRepository.deleteAllChatMessages(chatId)
            metaWriter.deleteRoom(chatId)
        }
        declarationsWriter.setConfig(chatId, config)
    }

    suspend fun setMetadata(extensionId: ChatExtensionId, chatId: ChatId, name: String, metadata: ChatRoomMetadataModel) =
        declare(extensionId, chatId) {
            chatRoomRepository.renameRoom(chatId, name)
            declarationsWriter.setMetadata(chatId, metadata)
        }

    suspend fun setPreview(extensionId: ChatExtensionId, chatId: ChatId, preview: ChatRoomPreviewModel) =
        declare(extensionId, chatId) { declarationsWriter.setPreview(chatId, preview) }

    suspend fun setToolbar(extensionId: ChatExtensionId, chatId: ChatId, actions: List<ChatToolbarActionModel>) =
        declare(extensionId, chatId) { declarationsWriter.setToolbar(chatId, actions) }

    suspend fun setSuggestions(extensionId: ChatExtensionId, chatId: ChatId, suggestions: List<ChatSuggestionModel>) =
        declare(extensionId, chatId) { declarationsWriter.setSuggestions(chatId, suggestions) }

    suspend fun setCommands(extensionId: ChatExtensionId, chatId: ChatId, commands: List<ChatCommandModel>) =
        declare(extensionId, chatId) { declarationsWriter.setCommands(chatId, commands) }

    suspend fun setFooter(extensionId: ChatExtensionId, chatId: ChatId, footer: ChatRoomFooterModel) =
        declare(extensionId, chatId) { declarationsWriter.setFooter(chatId, footer) }

    suspend fun setActivity(extensionId: ChatExtensionId, chatId: ChatId, activity: ChatRoomActivityModel?) =
        declare(extensionId, chatId) { activityWriter.set(chatId, activity) }

    private suspend fun declare(extensionId: ChatExtensionId, chatId: ChatId, write: suspend () -> Unit): Result<Unit> =
        runCancellableCatching {
            if (!chatId.isExtensionChat(extensionId) || !chatRoomRepository.roomExists(chatId)) throw UnknownChatRoomException()
            write()
        }
}
