package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import javax.inject.Inject

/** What the user does to the messages of a product room that the product must know (02-DESIGN §3.3.6, §3.3.9). */
class ProductRoomUserActions @Inject constructor(
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val chatMessageRepository: ChatMessageRepository,
) {
    suspend fun sendEvent(chatId: ChatId, event: ChatRoomUserEvent): Result<Unit> = runCatching {
        val extension = checkNotNull(chatExtensionRegistry.getExtensionForChat(chatId)) { "No extension serves the room" }
        extension.onUserEvent(chatId, event)
    }

    /**
     * In a host-store room the host keeps a hidden tombstone of the message, so its id stays taken, and sends
     * `UserDeleted` (02-DESIGN §19 item 4). In a product-source room the product deletes it through `user_action`.
     */
    suspend fun deleteMessage(chatId: ChatId, messageId: ChatMessageId, policy: ChatRoomPolicy): Result<Unit> {
        if (!policy.hostAppliesUserActions) {
            return productQuerying(chatId).flatMap { it.userAction(chatId, ChatUserActionModel.DeleteMessage(messageId)) }.map { }
        }

        return runCatching { chatMessageRepository.markMessageDeleted(messageId) }
            .flatMap { sendEvent(chatId, ChatRoomUserEvent.MessageDeleted(messageId)) }
    }

    /** Only a product-source room offers Retry (02-DESIGN §13 item 9). */
    suspend fun retry(chatId: ChatId, messageId: ChatMessageId): Result<Unit> {
        return productQuerying(chatId).flatMap { it.userAction(chatId, ChatUserActionModel.Retry(messageId)) }.map { }
    }

    private suspend fun productQuerying(chatId: ChatId): Result<ProductChatQuerying> = runCatching {
        val extension = checkNotNull(chatExtensionRegistry.getExtensionForChat(chatId)) { "No extension serves the room" }
        checkNotNull(extension.roomQuerying(chatId)) { "The product of the room keeps no messages" }
    }
}
