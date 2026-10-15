package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
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
    val footers = mutableListOf<Pair<ProductChatIdParameter, ChatRoomFooterModel>>()

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

    override suspend fun setRoomToolbar(chatIdParameter: ProductChatIdParameter, actions: List<ChatToolbarActionModel>) =
        notStubbed<Result<Unit>>()

    override suspend fun setRoomSuggestions(chatIdParameter: ProductChatIdParameter, suggestions: List<ChatSuggestionModel>) =
        notStubbed<Result<Unit>>()

    override suspend fun setRoomCommands(chatIdParameter: ProductChatIdParameter, commands: List<ChatCommandModel>) =
        notStubbed<Result<Unit>>()

    override suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, footer: ChatRoomFooterModel): Result<Unit> {
        footers += chatIdParameter to footer
        return Result.success(Unit)
    }

    override suspend fun setRoomActivity(chatIdParameter: ProductChatIdParameter, activity: ChatRoomActivityModel?) =
        notStubbed<Result<Unit>>()

    override suspend fun applyFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel) =
        notStubbed<Result<Unit>>()

    override suspend fun deliverLiveFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Boolean = false

    override suspend fun setRoomFeatures(chatIdParameter: ProductChatIdParameter, features: ChatRoomFeaturesModel) =
        notStubbed<Result<Unit>>()

    override suspend fun setMessageMeta(chatIdParameter: ProductChatIdParameter, messageId: String, meta: ChatMessageMetaModel) =
        notStubbed<Result<Unit>>()

    override suspend fun setRoomCall(chatIdParameter: ProductChatIdParameter, call: ChatRoomCallModel?) = notStubbed<Result<Unit>>()
}

private fun <T> notStubbed(): T = error("not stubbed by this test")
