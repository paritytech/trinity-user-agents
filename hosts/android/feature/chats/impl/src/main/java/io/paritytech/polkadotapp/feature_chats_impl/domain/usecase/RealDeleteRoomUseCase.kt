package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import javax.inject.Inject

/**
 * Removes a room with its messages, their reactions and revisions, its declarations (with the features and the
 * contact) and its call. A person chat of the chat product also loses its draft, as a native person chat does; the room
 * of any other product keeps it, as before the chat product.
 */
class RealDeleteRoomUseCase @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val chatDraftRepository: ChatDraftRepository,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
    private val callWriter: ChatRoomCallWriter,
) : DeleteRoomUseCase {
    override suspend fun invoke(chatId: ChatId) {
        val personChat = declarationsRepository.get(chatId)?.contact != null
        chatRoomRepository.deleteRoom(chatId)
        // The reactions and the revisions of a message go with it (foreign keys with cascade).
        chatMessageRepository.deleteAllChatMessages(chatId)
        if (personChat) chatDraftRepository.deleteDraft(chatId)
        declarationsWriter.delete(chatId)
        callWriter.set(chatId, null)
    }
}
