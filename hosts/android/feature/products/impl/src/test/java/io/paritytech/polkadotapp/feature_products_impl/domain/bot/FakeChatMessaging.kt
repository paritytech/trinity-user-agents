package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

class FakeChatMessaging(
    private val rooms: Flow<List<ProductChatRoom>> = emptyFlow(),
    private val onCreateRoom: suspend (CreateProductRoomRequest) -> Result<CreateProductRoomResult> = { notStubbed() },
    private val onSendMessage: suspend (ProductChatIdParameter, ProductBotMessage) -> Result<ChatMessageId> =
        { _, _ -> notStubbed() },
) : ProductChatMessaging {
    val sentText = mutableListOf<Pair<String, String>>()
    val footers = mutableListOf<Pair<ProductChatIdParameter, Boolean>>()

    override suspend fun createRoom(request: CreateProductRoomRequest): Result<CreateProductRoomResult> =
        onCreateRoom(request)

    override suspend fun sendMessage(
        chatIdParameter: ProductChatIdParameter,
        message: ProductBotMessage,
    ): Result<ChatMessageId> {
        if (message is ProductBotMessage.Text) sentText += chatIdParameter.value to message.text
        return onSendMessage(chatIdParameter, message)
    }

    override suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, showsTextInput: Boolean): Result<Unit> {
        footers += chatIdParameter to showsTextInput
        return Result.success(Unit)
    }

    override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> = rooms
}

private fun <T> notStubbed(): T = error("not stubbed by this test")
