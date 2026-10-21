package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import dagger.Lazy
import dagger.hilt.android.qualifiers.ApplicationContext
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductNotificationPublisher
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationHandler
import timber.log.Timber
import uniffi.truapi.ChatPushPreview
import uniffi.truapi.ChatWakeReason
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/** Opens the payload of a chat push with the key of its route (the core's A.15 decoder). */
fun interface ChatPushDecoder {
    fun decode(key: ByteArray, message: ByteArray): ChatPushPreview?
}

class CoreChatPushDecoder @Inject constructor() : ChatPushDecoder {
    override fun decode(key: ByteArray, message: ByteArray): ChatPushPreview? = uniffi.truapi.decodeChatPush(key, message)
}

/**
 * Shows the pushes of the chat product's contacts, which keep the native chat's shape (`pushId`, `message`), and
 * wakes the chat worker so it takes the message from the statement store. It is asked before the native chat's
 * handler, which would read the same fields.
 */
class ProductChatPushHandler @Inject constructor(
    private val routes: ProductChatPushRoutes,
    private val decoder: ChatPushDecoder,
    private val publisher: ProductNotificationPublisher,
    // Lazy: the runtime boots on first use, and a push must not boot it just to be checked.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    @param:ApplicationContext private val context: Context,
) : PushNotificationHandler {
    override val priority: Int = 1

    override suspend fun canHandle(data: Map<String, String>): Boolean = data[PUSH_ID]?.let(routes::find) != null

    override suspend fun handle(data: Map<String, String>) {
        val route = data[PUSH_ID]?.let(routes::find) ?: return
        val message = data[MESSAGE]?.let { runCatching { it.fromHex() }.getOrNull() } ?: return
        val text = when (val preview = decoder.decode(route.key, message)) {
            is ChatPushPreview.Message -> preview.text
            is ChatPushPreview.Call -> context.getString(
                if (preview.video) RCommon.string.product_chat_push_video_call else RCommon.string.product_chat_push_call,
            )
            null -> return
        }
        val productId = ProductId.fromStoredValue(route.productId)
        publisher.publishProductNotification(
            productId = productId,
            notificationId = route.pushId.hashCode(),
            text = text,
            deeplink = "polkadot://${route.productId}/-/chat/${route.roomId}",
        )
        wake(route)
    }

    private suspend fun wake(route: ProductChatPushRoute) {
        val runtime = runtimeProvider.get().runtime().getOrElse {
            Timber.w(it, "TrUAPI chat push: no runtime to wake %s", route.productId)
            return
        }
        runCatching { runtime.wakeProduct(route.productId, ChatWakeReason.Notification(route.roomId, WATCH_ID)) }
            .onFailure { Timber.w(it, "TrUAPI chat push: %s was not woken", route.productId) }
    }

    private companion object {
        const val PUSH_ID = "pushId"
        const val MESSAGE = "message"
        const val WATCH_ID = "chat-push"
    }
}
