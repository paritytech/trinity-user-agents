package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ChatHostBridge
import io.parity.truapi.HOST_DEFAULT_CHAT_ROOM_CONFIG
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import kotlinx.coroutines.flow.firstOrNull
import timber.log.Timber
import uniffi.truapi.ChatActivity
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatCommandSpec
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.ChatSuggestion
import uniffi.truapi.ChatToolbarAction
import uniffi.truapi.HostRejection

/**
 * Answers the core's chat callbacks for one product against its chat binding. The core has checked
 * that the product hosts each room and screened every value; the empty-id checks keep a broken call
 * away from the chat store.
 */
class ProductChatHostBridge(
    private val productId: ProductId,
    private val chatMessaging: ProductChatMessaging,
) : ChatHostBridge {
    override suspend fun createRoom(roomId: String, name: String, icon: String): ChatRoomRegistrationStatus {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat room needs an id")
        return chatMessaging.createRoom(CreateProductRoomRequest(ProductChatIdParameter(roomId), name, icon))
            .orReject()
            .status
            .toCoreStatus()
    }

    override suspend fun registerBot(botId: String, name: String, icon: String): ChatBotRegistrationStatus =
        throw HostRejection.Rejected("this host has no bot registry")

    override suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?): String {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat message needs a room")
        val message = content.toProductBotMessage(alt)
            ?: throw HostRejection.Rejected("this host cannot apply a ${content.javaClass.simpleName} message")
        return chatMessaging.sendMessage(ProductChatIdParameter(roomId), message).orReject()
    }

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) {
        val room = requireRoom(roomId)
        // A footer that the product draws waits for P8.6.
        if (footer is ChatRoomFooter.Custom) throw HostRejection.Rejected("this host does not draw a custom chat footer")
        chatMessaging.setRoomFooter(room, footer.toModel()).orReject()
    }

    override suspend fun listRooms(): List<ChatRoom> =
        chatMessaging.subscribeChatRooms().firstOrNull()?.map { it.toCoreChatRoom() } ?: emptyList()

    override suspend fun roomConfig(roomId: String): ChatRoomConfig =
        chatMessaging.roomDeclarations(requireRoom(roomId)).orReject()?.config?.toCore() ?: HOST_DEFAULT_CHAT_ROOM_CONFIG

    override suspend fun setRoomConfig(roomId: String, config: ChatRoomConfig) {
        chatMessaging.setRoomConfig(requireRoom(roomId), config.toModel()).orReject()
    }

    override suspend fun setRoomMetadata(roomId: String, metadata: ChatRoomMetadata) {
        chatMessaging.setRoomMetadata(requireRoom(roomId), metadata.name, metadata.toModel()).orReject()
    }

    override suspend fun setRoomPreview(roomId: String, preview: ChatRoomPreview) {
        chatMessaging.setRoomPreview(requireRoom(roomId), preview.toModel()).orReject()
    }

    override suspend fun deleteRoom(roomId: String) {
        chatMessaging.deleteRoom(requireRoom(roomId)).orReject()
    }

    override suspend fun setRoomToolbar(roomId: String, actions: List<ChatToolbarAction>) {
        chatMessaging.setRoomToolbar(requireRoom(roomId), actions.map { it.toModel() }).orReject()
    }

    override suspend fun setRoomSuggestions(roomId: String, suggestions: List<ChatSuggestion>) {
        chatMessaging.setRoomSuggestions(requireRoom(roomId), suggestions.map { it.toModel() }).orReject()
    }

    override suspend fun setRoomCommands(roomId: String, commands: List<ChatCommandSpec>) {
        chatMessaging.setRoomCommands(requireRoom(roomId), commands.map { it.toModel() }).orReject()
    }

    override suspend fun setRoomActivity(roomId: String, activity: ChatActivity?) {
        chatMessaging.setRoomActivity(requireRoom(roomId), activity?.toModel()).orReject()
    }

    private fun requireRoom(roomId: String): ProductChatIdParameter {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat room needs an id")
        return ProductChatIdParameter(roomId)
    }

    private fun <T> Result<T>.orReject(): T = getOrElse { failure ->
        when (failure) {
            is HostRejection -> throw failure
            is UnknownChatRoomException -> throw HostRejection.Rejected("no such chat room")
        }
        Timber.w(failure, "truapi.chat refused for %s", productId.value)
        throw HostRejection.Rejected(failure.message ?: failure.toString())
    }

    private fun CreateRoomStatus.toCoreStatus(): ChatRoomRegistrationStatus = when (this) {
        CreateRoomStatus.New -> ChatRoomRegistrationStatus.NEW
        CreateRoomStatus.Exists -> ChatRoomRegistrationStatus.EXISTS
    }
}
