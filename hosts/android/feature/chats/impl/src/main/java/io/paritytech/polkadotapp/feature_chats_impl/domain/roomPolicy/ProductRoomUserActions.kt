package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import javax.inject.Inject
import javax.inject.Singleton

/** What the user does in a product room that the product must know (02-DESIGN §3.3.6, §3.3.9). */
@Singleton
class ProductRoomUserActions @Inject constructor(
    private val chatExtensionRegistry: ChatExtensionRegistry,
) {
    // As a native contact action, the action goes through also when the user leaves the screen before the answer.
    private val actions = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /** An answer of the user to the contact of a room of the chat product: accept, decline, block, unblock or leave. */
    suspend fun contactAction(chatId: ChatId, action: ChatUserActionModel): Result<Unit> = actions.async {
        productQuerying(chatId).flatMap { it.userAction(chatId, action) }.map { }
    }.await()

    private suspend fun productQuerying(chatId: ChatId): Result<ProductChatQuerying> = runCatching {
        val extension = checkNotNull(chatExtensionRegistry.getExtensionForChat(chatId)) { "No extension serves the room" }
        checkNotNull(extension.roomQuerying(chatId)) { "The product of the room keeps no messages" }
    }
}
