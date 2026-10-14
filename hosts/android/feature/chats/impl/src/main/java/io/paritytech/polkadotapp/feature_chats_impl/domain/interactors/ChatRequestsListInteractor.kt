package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithRequestTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getContactResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.ChatRequestError
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

interface ChatRequestsListInteractor {
    fun subscribeChatRequests(): Flow<List<ContactWithRequestTimestamp>>

    suspend fun declineRequest(requestContactAccountId: AccountId): Result<Unit>
}

class RealChatRequestsListInteractor @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val incomingChatRequestProcessor: IncomingChatRequestProcessor,
    private val coroutineDispatchers: CoroutineDispatchers,
) : ChatRequestsListInteractor {
    override fun subscribeChatRequests(): Flow<List<ContactWithRequestTimestamp>> {
        return contactsRepository.subscribeContactsWithPendingIncomingRequests()
    }

    override suspend fun declineRequest(requestContactAccountId: AccountId): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            contactsRepository.getContactResult(requestContactAccountId)
                .mapError { ChatRequestError.ContactNotFound }
                .flatMap { incomingChatRequestProcessor.declineIncomingRequest(it) }
        }
    }
}
