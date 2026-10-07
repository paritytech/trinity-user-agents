package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.parity.truapi.ChatHostBridge
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatIdParameter
import kotlinx.coroutines.flow.first
import timber.log.Timber
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The chat each product's core worker is allowed to write to. A product's chat extension binds its
 * messaging while it serves the product on the core; the worker execution only ever holds the
 * [ChatHostBridge] from [bridgeFor], which reads the binding per call, so a worker that boots before
 * chat starts, or keeps running after it stops, is refused rather than writing into a chat nobody
 * serves.
 */
@Singleton
class TrUAPIChatSurfaces @Inject constructor() {
    private val bound = ConcurrentHashMap<ProductId, ProductChatMessaging>()

    fun bind(productId: ProductId, messaging: ProductChatMessaging) {
        bound[productId] = messaging
    }

    fun unbind(productId: ProductId, messaging: ProductChatMessaging) {
        bound.remove(productId, messaging)
    }

    fun bridgeFor(productId: ProductId): ChatHostBridge = ProductChatHostBridge(productId) { bound[productId] }
}

/**
 * The core's chat callbacks served from the product's native chat rooms. Only text and custom
 * messages have a native surface; bots have none, so registering one is refused.
 */
private class ProductChatHostBridge(
    private val productId: ProductId,
    private val messaging: () -> ProductChatMessaging?,
) : ChatHostBridge {
    override suspend fun createRoom(roomId: String, name: String, icon: String): ChatRoomRegistrationStatus {
        Timber.d("truapi.chat.createRoom %s %s", productId.value, roomId)
        val request = CreateProductRoomRequest(
            chatIdParameter = ProductChatIdParameter(roomId.requireRoom()),
            name = name.ifEmpty { null },
            icon = icon.ifEmpty { null },
        )

        return when (requireMessaging().createRoom(request).getOrElse { throw it }.status) {
            CreateRoomStatus.New -> ChatRoomRegistrationStatus.NEW
            CreateRoomStatus.Exists -> ChatRoomRegistrationStatus.EXISTS
        }
    }

    override suspend fun registerBot(botId: String, name: String, icon: String): ChatBotRegistrationStatus {
        Timber.d("truapi.chat.registerBot %s %s -> rejecting", productId.value, botId)
        throw HostRejection.Rejected("bot registration is not supported by this host")
    }

    // Logs the variant only: a message body is user content.
    override suspend fun postMessage(roomId: String, content: ChatMessageContent): String {
        Timber.d("truapi.chat.postMessage %s %s %s", productId.value, roomId, content::class.simpleName)
        val room = ProductChatIdParameter(roomId.requireRoom())
        val message = when (content) {
            is ChatMessageContent.Text -> ProductBotMessage.Text(content.text)
            is ChatMessageContent.Custom -> ProductBotMessage.Custom(content.v1.messageType, content.v1.payload.toDataByteArray())
            else -> throw HostRejection.Rejected("this host renders text and custom messages only")
        }

        return requireMessaging().sendMessage(room, message).getOrElse { throw it }
    }

    override suspend fun listRooms(): List<ChatRoom> =
        requireMessaging().subscribeChatRooms().first().map { room ->
            ChatRoom(roomId = room.roomId, participatingAs = ChatRoomParticipation.ROOM_HOST)
        }

    private fun requireMessaging(): ProductChatMessaging =
        messaging() ?: throw HostRejection.Rejected("chat is not served for ${productId.value} on this runtime")

    // A blank room has no chat id a render context could name.
    private fun String.requireRoom(): String = ifBlank { throw HostRejection.Rejected("a chat message needs a room") }}
