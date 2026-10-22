package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

/** A room of the chat product with its contact. [timestamp] is the time of its newest message, in milliseconds. */
data class ProductContactRoom(val chatId: ChatId, val contact: ChatRoomContactModel, val timestamp: Long)

/** The rooms of the chat product that the native chat would show as chat requests. */
@Singleton
class ProductContactRooms @Inject constructor(
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val chatExtensionRegistry: ChatExtensionRegistry,
) {
    // A declined request leaves the list and its count at once, as in the native chat, before the product deletes its room.
    private val declined = MutableStateFlow(emptySet<ChatId>())

    fun markDeclined(chatId: ChatId) = declined.update { it + chatId }

    /**
     * The requests of contacts that wait for the user, in the order the database keeps the rooms, as the native list
     * reads its requests without an order.
     */
    fun observeIncomingRequests(): Flow<List<ProductContactRoom>> = combine(
        declarationsRepository.observeContactRooms(),
        chatExtensionRegistry.observeActiveExtensions(),
        declined,
    ) { rooms, extensions, declined ->
        val activeIds = extensions.map { it.id }.toSet()
        rooms.mapNotNull { (chatId, declarations) ->
            if (chatId in declined) return@mapNotNull null
            val contact = declarations.contact?.takeIf { it.state == ChatContactStateModel.INCOMING } ?: return@mapNotNull null
            if (chatId.extensionOrNull()?.extensionId !in activeIds) return@mapNotNull null
            val preview = declarations.preview
            ProductContactRoom(chatId, contact, preview?.timestamp ?: preview?.lastMessage?.timestamp ?: 0)
        }
    }.distinctUntilChanged()
}
