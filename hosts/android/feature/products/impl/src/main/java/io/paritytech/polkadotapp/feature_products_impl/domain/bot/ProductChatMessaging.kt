package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import kotlinx.coroutines.flow.Flow

/**
 * The chat-specific slice of [ProductsBotApi] that only makes sense while a chat surface drives
 * the worker. Chat binds an implementation onto the shared worker when it attaches and unbinds on
 * detach; a headless worker (kept alive only by the funding screen or an operation) has none bound.
 */
interface ProductChatMessaging {
    suspend fun createRoom(request: CreateProductRoomRequest): Result<CreateProductRoomResult>

    suspend fun sendMessage(chatIdParameter: ProductChatIdParameter, message: ProductBotMessage): Result<ChatMessageId>

    /** Sets what [chatIdParameter] shows below its messages, the core's room footer. */
    suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, footer: ProductRoomFooter): Result<Unit>

    fun subscribeChatRooms(): Flow<List<ProductChatRoom>>
}

/** What a product room shows below its messages. */
sealed interface ProductRoomFooter {
    data object TextInput : ProductRoomFooter

    data object Empty : ProductRoomFooter

    /** A body the product draws itself, docked under the messages and never shown as a message. */
    class Card(val messageType: String, val payload: ByteArray) : ProductRoomFooter {
        override fun equals(other: Any?): Boolean =
            other is Card && messageType == other.messageType && payload.contentEquals(other.payload)

        override fun hashCode(): Int = 31 * messageType.hashCode() + payload.contentHashCode()
    }
}
