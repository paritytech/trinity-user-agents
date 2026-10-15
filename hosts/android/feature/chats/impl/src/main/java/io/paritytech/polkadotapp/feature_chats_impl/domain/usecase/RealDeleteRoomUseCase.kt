package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatBotPersonaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatMessageMetaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatThreadRoomCascade
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.flow.first
import javax.inject.Inject

/**
 * Removes a room with its messages, their reactions, revisions and meta, its draft, its declarations (with the
 * features and the user state), its activity line and its call. The thread rooms of the room go with it in the same
 * transaction, so no thread stays hidden for ever (07-LATER §9.2).
 */
class RealDeleteRoomUseCase @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val chatDraftRepository: ChatDraftRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
    private val activityWriter: ChatRoomActivityWriter,
    private val metaWriter: ChatMessageMetaWriter,
    private val callWriter: ChatRoomCallWriter,
    private val personaWriter: ChatBotPersonaWriter,
    private val threadRooms: ChatThreadRoomCascade,
) : DeleteRoomUseCase {
    override suspend fun invoke(chatId: ChatId) {
        val threads = threadRooms.threadRoomsOf(chatId)
        threadRooms.inTransaction {
            (threads + chatId).forEach { deleteOne(it) }
        }
    }

    private suspend fun deleteOne(chatId: ChatId) {
        chatRoomRepository.deleteRoom(chatId)
        // The reactions and the revisions of a message go with it (foreign keys with cascade).
        chatMessageRepository.deleteAllChatMessages(chatId)
        chatDraftRepository.deleteDraft(chatId)
        declarationsWriter.delete(chatId)
        activityWriter.set(chatId, null)
        metaWriter.deleteRoom(chatId)
        callWriter.set(chatId, null)
    }

    override suspend fun deleteAllRoomsOf(extensionId: ChatExtensionId) {
        chatRoomRepository.subscribeRoomsByExtension(extensionId).first().forEach { invoke(it) }
        personaWriter.deleteAll(extensionId)
    }
}
