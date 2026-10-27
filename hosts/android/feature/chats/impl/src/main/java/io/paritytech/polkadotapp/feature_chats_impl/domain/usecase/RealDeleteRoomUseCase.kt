package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import javax.inject.Inject

/** Removes a room with its messages and the declarations of its product. */
class RealDeleteRoomUseCase @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val declarationsWriter: ChatRoomDeclarationsWriter,
) : DeleteRoomUseCase {
    override suspend fun invoke(chatId: ChatId) {
        chatRoomRepository.deleteRoom(chatId)
        chatMessageRepository.deleteAllChatMessages(chatId)
        declarationsWriter.delete(chatId)
    }
}
