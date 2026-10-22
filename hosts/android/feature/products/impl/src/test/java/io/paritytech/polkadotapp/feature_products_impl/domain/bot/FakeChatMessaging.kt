package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
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

    override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> = rooms

    override suspend fun roomDeclarations(chatIdParameter: ProductChatIdParameter): Result<ChatRoomDeclarations?> =
        Result.success(null)

    override suspend fun setRoomConfig(chatIdParameter: ProductChatIdParameter, config: ChatRoomConfigModel) = notStubbed<Result<Unit>>()

    override suspend fun setRoomMetadata(chatIdParameter: ProductChatIdParameter, name: String, metadata: ChatRoomMetadataModel) =
        notStubbed<Result<Unit>>()

    override suspend fun setRoomPreview(chatIdParameter: ProductChatIdParameter, preview: ChatRoomPreviewModel) =
        notStubbed<Result<Unit>>()

    override suspend fun deleteRoom(chatIdParameter: ProductChatIdParameter) = notStubbed<Result<Unit>>()

    override suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, showsTextInput: Boolean): Result<Unit> {
        footers += chatIdParameter to showsTextInput
        return Result.success(Unit)
    }

    override suspend fun applyFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel) =
        notStubbed<Result<Unit>>()

    override suspend fun deliverLiveFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Boolean = false

    override suspend fun setRoomFeatures(chatIdParameter: ProductChatIdParameter, features: ChatRoomFeaturesModel) =
        notStubbed<Result<Unit>>()

    override suspend fun setRoomCall(chatIdParameter: ProductChatIdParameter, call: ChatRoomCallModel?) = notStubbed<Result<Unit>>()

    val contacts = mutableListOf<Pair<ProductChatIdParameter, ChatRoomContactModel>>()

    override suspend fun setRoomContact(chatIdParameter: ProductChatIdParameter, contact: ChatRoomContactModel): Result<Unit> {
        contacts += chatIdParameter to contact
        return Result.success(Unit)
    }

    var ringingClears = 0

    override suspend fun clearRingingCalls(): Result<Unit> {
        ringingClears++
        return Result.success(Unit)
    }
}

private fun <T> notStubbed(): T = error("not stubbed by this test")
