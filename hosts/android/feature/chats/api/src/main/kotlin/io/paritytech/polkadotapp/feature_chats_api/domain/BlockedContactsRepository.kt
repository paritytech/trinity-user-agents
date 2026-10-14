package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import kotlinx.coroutines.flow.Flow

interface BlockedContactsRepository {
    fun subscribeHasBlockedContacts(): Flow<Boolean>
    fun subscribeBlockedContacts(): Flow<List<Contact>>
    suspend fun setBlocked(accountId: AccountId, isBlocked: Boolean)
}
