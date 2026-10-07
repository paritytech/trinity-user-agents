package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.parity.truapi.ChatHostBridge
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatIdParameter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * The chat each product's core worker is allowed to write to. A product's chat extension binds its
 * messaging while it serves the product on the core; the worker execution only ever holds the
 * [ChatHostBridge] from [bridgeFor], which reads the binding per call.
 *
 * Another surface, a Pocket card, can boot the same worker before chat starts, and the worker makes
 * its chat calls once, on start. So a call waits up to [bindWait] for the binding; one that still
 * finds none, because the product's chat is not running, is refused rather than written into a chat
 * nobody serves.
 */
@Singleton
class TrUAPIChatSurfaces(private val bindWait: Duration) {
    @Inject
    constructor() : this(BIND_WAIT)

    private val bound = MutableStateFlow<Map<ProductId, ProductChatMessaging>>(emptyMap())

    fun bind(productId: ProductId, messaging: ProductChatMessaging) {
        bound.update { it + (productId to messaging) }
    }

    fun unbind(productId: ProductId, messaging: ProductChatMessaging) {
        bound.update { if (it[productId] === messaging) it - productId else it }
    }

    fun bridgeFor(productId: ProductId): ChatHostBridge = ProductChatHostBridge(productId) {
        bound.value[productId] ?: withTimeoutOrNull(bindWait) { bound.mapNotNull { it[productId] }.first() }
    }

    private companion object {
        // Covers a chat extension that starts while a card has already booted the worker.
        val BIND_WAIT = 30.seconds
    }
}

/**
 * The core's chat callbacks served from the product's native chat rooms. Only text and custom
 * messages have a native surface; bots have none, so registering one is refused.
 */
private class ProductChatHostBridge(
    private val productId: ProductId,
    private val messaging: suspend () -> ProductChatMessaging?,
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

    private suspend fun requireMessaging(): ProductChatMessaging =
        messaging() ?: throw HostRejection.Rejected("chat is not served for ${productId.value} on this runtime")

    // A blank room has no chat id a render context could name.
    private fun String.requireRoom(): String = ifBlank { throw HostRejection.Rejected("a chat message needs a room") }}
