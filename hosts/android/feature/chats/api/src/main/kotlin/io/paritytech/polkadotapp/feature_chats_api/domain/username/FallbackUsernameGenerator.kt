package io.paritytech.polkadotapp.feature_chats_api.domain.username

import io.paritytech.polkadotapp.common.domain.model.AccountId

interface FallbackUsernameGenerator {
    fun generateFromAccountId(accountId: AccountId): String
}
