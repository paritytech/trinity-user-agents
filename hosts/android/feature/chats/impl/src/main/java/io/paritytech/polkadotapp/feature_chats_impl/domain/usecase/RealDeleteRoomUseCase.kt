package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.flow.first
import javax.inject.Inject

/** Removes a room with its messages, their reactions and revisions, its draft, its declarations and its activity line. */
class RealDeleteRoomUseCase @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val chatDraftRepository: ChatDraftRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
    private val activityWriter: ChatRoomActivityWriter,
) : DeleteRoomUseCase {
    override suspend fun invoke(chatId: ChatId) {
        chatRoomRepository.deleteRoom(chatId)
        // The reactions and the revisions of a message go with it (foreign keys with cascade).
        chatMessageRepository.deleteAllChatMessages(chatId)
        chatDraftRepository.deleteDraft(chatId)
        declarationsWriter.delete(chatId)
        activityWriter.set(chatId, null)
    }

    override suspend fun deleteAllRoomsOf(extensionId: ChatExtensionId) {
        chatRoomRepository.subscribeRoomsByExtension(extensionId).first().forEach { invoke(it) }
    }
}
