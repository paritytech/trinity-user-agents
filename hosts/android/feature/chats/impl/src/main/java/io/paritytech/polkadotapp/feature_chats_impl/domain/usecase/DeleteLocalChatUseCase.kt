package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.RemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onContact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import javax.inject.Inject

class DeleteLocalChatUseCase @Inject constructor(
    private val chatMessageRepository: ChatMessageRepository,
    private val removeContactUseCase: RemoveContactUseCase,
    private val chatDraftRepository: ChatDraftRepository,
) {
    suspend operator fun invoke(chatId: ChatId) {
        chatMessageRepository.deleteAllChatMessages(chatId)
        chatId.onContact { contactChat -> removeContactUseCase.removeContact(contactChat.contactAccountId) }
        chatDraftRepository.deleteDraft(chatId)
    }
}
