package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaLimitException
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRegistry
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotRegistrationModel
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.productRoomId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import kotlinx.coroutines.flow.first
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatRoomUserState
import uniffi.truapi.HostRejection
import javax.inject.Inject

/**
 * Reads what the host keeps for the chat of a product without a chat surface: the user state of its rooms and its
 * bot personas. The core reads them when a worker subscribes to its room events, also for a worker that runs
 * without a chat screen. It also keeps the personas that the product registers.
 */
class ProductChatStoredState @Inject constructor(
    private val userStates: ChatRoomUserStateRepository,
    private val personas: ChatBotPersonaRepository,
    private val personaRegistry: ChatBotPersonaRegistry,
) {
    suspend fun roomUserStates(productId: ProductId): List<ChatRoomUserState> {
        val extensionId = productId.toChatExtensionId()
        return userStates.observeAll().first().mapNotNull { (chatId, state) ->
            chatId.productRoomId(extensionId)?.let { room -> state.toCore(room.value) }
        }
    }

    suspend fun botIds(productId: ProductId): List<String> =
        personas.observe(productId.toChatExtensionId()).first().map { it.botId }

    /**
     * Keeps the persona `bot:<botId>` of the product (07-LATER §5.2). A repeat gives the persona the new name and
     * icon. The core screened the values and the bound already; the store checks the bound again in its transaction.
     */
    suspend fun registerBot(productId: ProductId, botId: String, name: String, icon: String): ChatBotRegistrationStatus {
        if (botId.isEmpty()) throw HostRejection.Rejected("a bot needs an id")
        val registration = try {
            personaRegistry.register(productId.toChatExtensionId(), ChatBotPersonaModel(botId, name, icon))
        } catch (limit: ChatBotPersonaLimitException) {
            throw HostRejection.Rejected(limit.message ?: limit.toString())
        }
        return when (registration) {
            ChatBotRegistrationModel.NEW -> ChatBotRegistrationStatus.NEW
            ChatBotRegistrationModel.EXISTS -> ChatBotRegistrationStatus.EXISTS
        }
    }
}

/**
 * Sends the room events of the Chat Extension v2 additions to a product through the core. The core keeps the newest
 * events of a product until its worker subscribes, so a publish never starts the worker.
 */
interface ProductChatRoomEventSink {
    suspend fun publish(productId: ProductId, room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit>
}

class TrUAPIChatRoomEventSink @Inject constructor(
    // Lazy: the runtime provider reaches the worker supervisor, which reaches the chat extensions.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
) : ProductChatRoomEventSink {
    override suspend fun publish(productId: ProductId, room: ProductChatIdParameter, event: ChatRoomEventModel): Result<Unit> =
        runtimeProvider.get().runtime().flatMap { runtime ->
            runCancellableCatching { runtime.publishChatRoomEvent(productId.value, room.value, event.toCore()) }
        }
}
