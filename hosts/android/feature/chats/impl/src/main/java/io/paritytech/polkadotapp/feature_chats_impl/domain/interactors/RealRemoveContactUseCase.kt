package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.RemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RemovedChatsRepository
import javax.inject.Inject
import kotlin.time.Instant

class RealRemoveContactUseCase @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val removedChatsRepository: RemovedChatsRepository,
) : RemoveContactUseCase {
    override suspend fun removeContact(accountId: AccountId) {
        contactsRepository.deleteContact(accountId)
        removedChatsRepository.recordRemoval(accountId, System.currentTimeMillis())
    }

    override suspend fun recordRemoteContactRemoval(accountId: AccountId, removedAt: Long) {
        contactsRepository.deleteContact(accountId)
        removedChatsRepository.recordRemoval(accountId, removedAt)
    }

    override suspend fun getRemovedContactsAfter(after: Instant): List<AccountId> {
        return removedChatsRepository.getRemovedAfter(after)
    }
}
