package io.paritytech.polkadotapp.feature_chats_impl

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.feature_chats_api.presentation.ChatStarter
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.StartChatDataUseCase
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.toChatFeedPayload
import javax.inject.Inject

class RealChatStarter @Inject constructor(
    private val startChatDataUseCase: StartChatDataUseCase,
    private val chatsRouter: ChatsRouter
) : ChatStarter {
    override suspend fun openChatWith(accountId: AccountId): Result<Unit> {
        return startChatDataUseCase.invoke(accountId)
            .map { it.toChatFeedPayload() }
            .onSuccess { chatsRouter.openChatFeed(it) }
            .coerceToUnit()
    }
}
