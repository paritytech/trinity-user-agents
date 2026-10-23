package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductContactRooms
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductRoomUserActions
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** A chat request that waits for the user. [productRoom] is the room of the chat product that holds it. */
data class PendingChatRequest(
    val accountId: AccountId,
    val display: ChatDisplay,
    val timestamp: Long,
    val productRoom: ChatId,
)

interface ChatRequestsListInteractor {
    fun subscribeChatRequests(): Flow<List<PendingChatRequest>>

    /** [productRoom] is the room of the chat product that holds the request. */
    suspend fun declineRequest(productRoom: ChatId): Result<Unit>
}

class RealChatRequestsListInteractor @Inject constructor(
    private val chatEngine: ChatEngine,
    private val productContactRooms: ProductContactRooms,
    private val productRoomUserActions: ProductRoomUserActions,
) : ChatRequestsListInteractor {
    // The rooms of the chat product hold the requests.
    override fun subscribeChatRequests(): Flow<List<PendingChatRequest>> =
        productContactRooms.observeIncomingRequests().map { rooms ->
            rooms.map { PendingChatRequest(it.contact.account, chatEngine.getContactRoomDisplay(it.contact), it.timestamp, it.chatId) }
        }

    override suspend fun declineRequest(productRoom: ChatId): Result<Unit> =
        productRoomUserActions.contactAction(productRoom, ChatUserActionModel.DeclineRequest)
            .onSuccess { productContactRooms.markDeclined(productRoom) }
}
