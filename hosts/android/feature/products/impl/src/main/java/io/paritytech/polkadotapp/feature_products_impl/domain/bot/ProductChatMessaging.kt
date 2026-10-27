package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
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

    // Chat Extension v2. A room that the product does not host fails with `UnknownChatRoomException`.

    /** What the product declared for the room. Null when it declared nothing. */
    suspend fun roomDeclarations(chatIdParameter: ProductChatIdParameter): Result<ChatRoomDeclarations?>

    suspend fun setRoomConfig(chatIdParameter: ProductChatIdParameter, config: ChatRoomConfigModel): Result<Unit>

    suspend fun setRoomMetadata(chatIdParameter: ProductChatIdParameter, name: String, metadata: ChatRoomMetadataModel): Result<Unit>

    suspend fun setRoomPreview(chatIdParameter: ProductChatIdParameter, preview: ChatRoomPreviewModel): Result<Unit>

    /** Removes the room with everything the host keeps for it. A room that is gone already is not an error. */
    suspend fun deleteRoom(chatIdParameter: ProductChatIdParameter): Result<Unit>

    /** Stores a feed update of a room that the host keeps. A refusal is a `ProductFeedRejection`. */
    suspend fun applyFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Result<Unit>

    /** Gives a feed update of a room that the product keeps to the screens that show it. False when none does. */
    suspend fun deliverLiveFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Boolean
}
