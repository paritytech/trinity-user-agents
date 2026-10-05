package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.ChatHostBridge
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatIdParameter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection

class TrUAPIWorkerChat : ChatHostBridge {
    private val messagingState = MutableStateFlow<ProductChatMessaging?>(null)
    var messaging: ProductChatMessaging?
        get() = messagingState.value
        set(value) { messagingState.value = value }

    private suspend fun messaging(): ProductChatMessaging = messagingState.filterNotNull().first()

    override suspend fun createRoom(roomId: String, name: String, icon: String): ChatRoomRegistrationStatus =
        when (messaging().createRoom(CreateProductRoomRequest(ProductChatIdParameter(roomId), name, icon)).getOrThrow().status) {
            CreateRoomStatus.New -> ChatRoomRegistrationStatus.NEW
            CreateRoomStatus.Exists -> ChatRoomRegistrationStatus.EXISTS
        }

    override suspend fun registerBot(botId: String, name: String, icon: String): ChatBotRegistrationStatus =
        throw HostRejection.Rejected("Bot registration is not supported by this host")

    override suspend fun postMessage(roomId: String, content: ChatMessageContent): String {
        val message = when (content) {
            is ChatMessageContent.Text -> ProductBotMessage.Text(content.text)
            is ChatMessageContent.Custom -> ProductBotMessage.Custom(content.v1.messageType, DataByteArray(content.v1.payload))
            else -> throw HostRejection.Rejected("This host renders text and custom messages only")
        }
        return messaging().sendMessage(ProductChatIdParameter(roomId), message).getOrThrow()
    }

    override suspend fun listRooms(): List<ChatRoom> = messaging().subscribeChatRooms().first().map {
        ChatRoom(it.roomId, if (it.participatingAs == "Bot") ChatRoomParticipation.BOT else ChatRoomParticipation.ROOM_HOST)
    }
}
