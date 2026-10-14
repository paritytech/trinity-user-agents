package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import javax.inject.Inject

class RealDeleteRoomUseCase @Inject constructor(
    private val chatRoomRepository: ChatRoomRepository,
    private val chatMessageRepository: ChatMessageRepository
) : DeleteRoomUseCase {
    override suspend fun invoke(chatId: ChatId) {
        chatRoomRepository.deleteRoom(chatId)
        chatMessageRepository.deleteAllChatMessages(chatId)
    }
}
