package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import kotlinx.coroutines.flow.first
import javax.inject.Inject

interface WaitForChatExistsUseCase {
    context(scope: ComputationalScope)
    suspend operator fun invoke(chatId: ChatId)
}

class RealWaitForChatExistsUseCase @Inject constructor(
    private val chatEngine: ChatEngine,
) : WaitForChatExistsUseCase {
    context(scope: ComputationalScope)
    override suspend operator fun invoke(chatId: ChatId) {
        // The chat list does not show a thread room (07-LATER §9.2), so the rooms of a product decide for its rooms.
        if (chatId.extensionOrNull() != null) {
            chatEngine.observeRoomExists(chatId).first { it }
            return
        }
        chatEngine.subscribeChatSummaries()
            .first { summaries -> summaries.any { it.chatId == chatId } }
    }
}
