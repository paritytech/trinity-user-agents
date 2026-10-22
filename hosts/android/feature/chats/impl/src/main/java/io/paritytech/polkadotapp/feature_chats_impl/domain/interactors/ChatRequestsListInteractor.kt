package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getContactResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.ChatRequestError
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductContactRooms
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductRoomUserActions
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import javax.inject.Inject

/** A chat request that waits for the user. [productRoom] is the room of the chat product that holds it, or null. */
data class PendingChatRequest(
    val accountId: AccountId,
    val display: ChatDisplay,
    val timestamp: Long,
    val productRoom: ChatId?,
)

interface ChatRequestsListInteractor {
    fun subscribeChatRequests(): Flow<List<PendingChatRequest>>

    /** [productRoom] is the room of the chat product that holds the request, or null for a request of the native chat. */
    suspend fun declineRequest(requestContactAccountId: AccountId, productRoom: ChatId?): Result<Unit>
}

class RealChatRequestsListInteractor @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val incomingChatRequestProcessor: IncomingChatRequestProcessor,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val chatEngine: ChatEngine,
    private val personChatMode: PersonChatMode,
    private val productContactRooms: ProductContactRooms,
    private val productRoomUserActions: ProductRoomUserActions,
) : ChatRequestsListInteractor {
    // While the chat product runs the chats, its rooms hold the requests.
    override fun subscribeChatRequests(): Flow<List<PendingChatRequest>> = if (personChatMode.isProductChat()) {
        productContactRooms.observeIncomingRequests().map { rooms ->
            rooms.map { PendingChatRequest(it.contact.account, chatEngine.getContactRoomDisplay(it.contact), it.timestamp, it.chatId) }
        }
    } else {
        contactsRepository.subscribeContactsWithPendingIncomingRequests().map { requests ->
            requests.map { PendingChatRequest(it.contact.accountId, chatEngine.getContactChatDisplay(it.contact), it.requestTimestamp, null) }
        }
    }

    override suspend fun declineRequest(requestContactAccountId: AccountId, productRoom: ChatId?): Result<Unit> {
        if (productRoom != null) {
            return productRoomUserActions.contactAction(productRoom, ChatUserActionModel.DeclineRequest)
                .onSuccess { productContactRooms.markDeclined(productRoom) }
        }

        return withContext(coroutineDispatchers.io) {
            contactsRepository.getContactResult(requestContactAccountId)
                .mapError { ChatRequestError.ContactNotFound }
                .flatMap { incomingChatRequestProcessor.declineIncomingRequest(it) }
        }
    }
}
