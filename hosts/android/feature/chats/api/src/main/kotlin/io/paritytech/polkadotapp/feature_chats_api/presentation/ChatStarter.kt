package io.paritytech.polkadotapp.feature_chats_api.presentation

import io.paritytech.polkadotapp.common.domain.model.AccountId

interface ChatStarter {
    suspend fun openChatWith(accountId: AccountId): Result<Unit>
}
