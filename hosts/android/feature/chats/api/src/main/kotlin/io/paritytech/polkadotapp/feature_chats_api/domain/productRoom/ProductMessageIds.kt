package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

/**
 * The store gives each message one id across all chats, but a product picks the id of a message in one
 * room only. So the store keeps a message that a product pushes under an id that starts with its room. A
 * message that the host made (a message of the user, a `post_message` message) keeps the id that the host
 * gave it, and the product knows it under that id.
 */
object ProductMessageIds {
    fun stored(productMessageId: String, chatId: ChatId): ChatMessageId = prefix(chatId) + productMessageId

    /** The id under which the product knows a stored message. */
    fun product(storedId: ChatMessageId, chatId: ChatId): String = storedId.removePrefix(prefix(chatId))

    /** The stored ids that a product id can name, the id of a pushed message first. */
    fun candidates(productMessageId: String, chatId: ChatId): List<ChatMessageId> =
        listOf(stored(productMessageId, chatId), productMessageId)

    private fun prefix(chatId: ChatId) = chatId.value.value.decodeToString() + "/"
}
