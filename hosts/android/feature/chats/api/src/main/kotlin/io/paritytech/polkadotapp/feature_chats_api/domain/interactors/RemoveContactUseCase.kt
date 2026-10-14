package io.paritytech.polkadotapp.feature_chats_api.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlin.time.Instant

interface RemoveContactUseCase {
    suspend fun removeContact(accountId: AccountId)

    suspend fun recordRemoteContactRemoval(accountId: AccountId, removedAt: Long)

    suspend fun getRemovedContactsAfter(after: Instant): List<AccountId>
}
