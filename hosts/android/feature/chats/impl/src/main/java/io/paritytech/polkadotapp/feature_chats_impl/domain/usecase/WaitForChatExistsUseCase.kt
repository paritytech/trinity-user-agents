package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
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
        chatEngine.subscribeChatSummaries()
            .first { summaries -> summaries.any { it.chatId == chatId } }
    }
}
