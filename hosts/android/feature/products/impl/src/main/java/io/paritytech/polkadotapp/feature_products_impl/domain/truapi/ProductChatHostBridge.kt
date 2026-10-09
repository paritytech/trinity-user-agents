package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ChatHostBridge
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import kotlinx.coroutines.flow.firstOrNull
import timber.log.Timber
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection

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

    override suspend fun postMessage(roomId: String, content: ChatMessageContent): String {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat message needs a room")
        val message = content.toProductBotMessage()
            ?: throw HostRejection.Rejected(
                "this host cannot render a ${content.javaClass.simpleName} message",
            )
        return chatMessaging.sendMessage(ProductChatIdParameter(roomId), message).orReject()
    }

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) {
        if (roomId.isEmpty()) throw HostRejection.Rejected("a chat room needs an id")
        val showsTextInput = when (footer) {
            ChatRoomFooter.TEXT_INPUT -> true
            ChatRoomFooter.EMPTY -> false
        }
        chatMessaging.setRoomFooter(ProductChatIdParameter(roomId), showsTextInput).orReject()
    }

    override suspend fun listRooms(): List<ChatRoom> =
        chatMessaging.subscribeChatRooms().firstOrNull()?.map { it.toCoreChatRoom() } ?: emptyList()

    private fun <T> Result<T>.orReject(): T = getOrElse { failure ->
        if (failure is HostRejection) throw failure
        Timber.w(failure, "truapi.chat refused for %s", productId.value)
        throw HostRejection.Rejected(failure.message ?: failure.toString())
    }

    private fun ChatMessageContent.toProductBotMessage(): ProductBotMessage? = when (this) {
        is ChatMessageContent.Text -> ProductBotMessage.Text(text)
        is ChatMessageContent.Custom -> ProductBotMessage.Custom(v1.messageType, v1.payload.toDataByteArray())
        is ChatMessageContent.RichText,
        is ChatMessageContent.Actions,
        is ChatMessageContent.File,
        is ChatMessageContent.Reaction,
        is ChatMessageContent.ReactionRemoved,
        -> null
    }

    private fun CreateRoomStatus.toCoreStatus(): ChatRoomRegistrationStatus = when (this) {
        CreateRoomStatus.New -> ChatRoomRegistrationStatus.NEW
        CreateRoomStatus.Exists -> ChatRoomRegistrationStatus.EXISTS
    }
}
