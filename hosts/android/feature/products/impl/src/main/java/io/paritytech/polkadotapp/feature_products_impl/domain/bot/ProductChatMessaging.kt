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

    /** Sets whether [chatIdParameter] shows the text input below its messages; the core's room footer. */
    suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, showsTextInput: Boolean): Result<Unit>

    fun subscribeChatRooms(): Flow<List<ProductChatRoom>>
}
