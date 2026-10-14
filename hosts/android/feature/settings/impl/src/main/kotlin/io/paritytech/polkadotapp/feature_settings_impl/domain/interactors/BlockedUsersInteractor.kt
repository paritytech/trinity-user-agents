package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.BlockedContactsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class BlockedUsersInteractor @Inject constructor(private val repository: BlockedContactsRepository) {
    fun subscribeBlockedContacts(): Flow<List<Contact>> = repository.subscribeBlockedContacts()

    suspend fun unblockContact(accountId: AccountId) = repository.setBlocked(accountId, false)
}
