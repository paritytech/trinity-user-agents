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
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ModalityApiSlot
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.WeakModalityApiSlot
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf

private const val MESSAGING_NOT_SUPPORTED_CODE = "messaging_not_supported"

// The ProductsBotApi a shared worker boots with. General host calls always work through
// BaseProductsBotApi; the chat-messaging calls route to whatever ProductChatMessaging is bound in
// chatSlot, and fail cleanly when nothing is bound.
class BindableProductsBotApi(
    hostApiInteractor: HostApiInteractor,
    callingProductIdProvider: CallingProductIdProvider,
) : BaseProductsBotApi(hostApiInteractor, callingProductIdProvider), ProductChatMessaging {
    val chatSlot: ModalityApiSlot<ProductChatMessaging> = WeakModalityApiSlot()

    override suspend fun createRoom(request: CreateProductRoomRequest): Result<CreateProductRoomResult> {
        return chatSlot.tryUse().fold(
            onSuccess = { it.createRoom(request) },
            onFailure = { messagingNotSupported() },
        )
    }

    override suspend fun sendMessage(
        chatIdParameter: ProductChatIdParameter,
        message: ProductBotMessage,
    ): Result<ChatMessageId> {
        return chatSlot.tryUse().fold(
            onSuccess = { it.sendMessage(chatIdParameter, message) },
            onFailure = { messagingNotSupported() },
        )
    }

    override suspend fun roomDeclarations(chatIdParameter: ProductChatIdParameter): Result<ChatRoomDeclarations?> =
        withChat { it.roomDeclarations(chatIdParameter) }

    override suspend fun setRoomConfig(chatIdParameter: ProductChatIdParameter, config: ChatRoomConfigModel): Result<Unit> =
        withChat { it.setRoomConfig(chatIdParameter, config) }

    override suspend fun setRoomMetadata(
        chatIdParameter: ProductChatIdParameter,
        name: String,
        metadata: ChatRoomMetadataModel,
    ): Result<Unit> = withChat { it.setRoomMetadata(chatIdParameter, name, metadata) }

    override suspend fun setRoomPreview(chatIdParameter: ProductChatIdParameter, preview: ChatRoomPreviewModel): Result<Unit> =
        withChat { it.setRoomPreview(chatIdParameter, preview) }

    override suspend fun deleteRoom(chatIdParameter: ProductChatIdParameter): Result<Unit> =
        withChat { it.deleteRoom(chatIdParameter) }

    override suspend fun setRoomToolbar(chatIdParameter: ProductChatIdParameter, actions: List<ChatToolbarActionModel>): Result<Unit> =
        withChat { it.setRoomToolbar(chatIdParameter, actions) }

    override suspend fun setRoomSuggestions(
        chatIdParameter: ProductChatIdParameter,
        suggestions: List<ChatSuggestionModel>,
    ): Result<Unit> = withChat { it.setRoomSuggestions(chatIdParameter, suggestions) }

    override suspend fun setRoomCommands(chatIdParameter: ProductChatIdParameter, commands: List<ChatCommandModel>): Result<Unit> =
        withChat { it.setRoomCommands(chatIdParameter, commands) }

    override suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, footer: ChatRoomFooterModel): Result<Unit> =
        withChat { it.setRoomFooter(chatIdParameter, footer) }

    override suspend fun setRoomActivity(chatIdParameter: ProductChatIdParameter, activity: ChatRoomActivityModel?): Result<Unit> =
        withChat { it.setRoomActivity(chatIdParameter, activity) }

    override suspend fun applyFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Result<Unit> =
        withChat { it.applyFeedUpdate(chatIdParameter, update) }

    override suspend fun deliverLiveFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Boolean =
        chatSlot.tryUse().getOrNull()?.deliverLiveFeedUpdate(chatIdParameter, update) ?: false

    override suspend fun setRoomFeatures(chatIdParameter: ProductChatIdParameter, features: ChatRoomFeaturesModel): Result<Unit> =
        withChat { it.setRoomFeatures(chatIdParameter, features) }

    override suspend fun setMessageMeta(
        chatIdParameter: ProductChatIdParameter,
        messageId: String,
        meta: ChatMessageMetaModel,
    ): Result<Unit> = withChat { it.setMessageMeta(chatIdParameter, messageId, meta) }

    override suspend fun setRoomCall(chatIdParameter: ProductChatIdParameter, call: ChatRoomCallModel?): Result<Unit> =
        withChat { it.setRoomCall(chatIdParameter, call) }

    private suspend fun <T> withChat(call: suspend (ProductChatMessaging) -> Result<T>): Result<T> =
        chatSlot.tryUse().fold(
            onSuccess = { call(it) },
            onFailure = { messagingNotSupported() },
        )

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> {
        return chatSlot.bound.flatMapLatest { it?.subscribeChatRooms() ?: flowOf(emptyList()) }
    }

    private fun <T> messagingNotSupported(): Result<T> = Result.failure(
        HostCallException(MESSAGING_NOT_SUPPORTED_CODE, "Product messaging is not supported in this context")
    )
}
