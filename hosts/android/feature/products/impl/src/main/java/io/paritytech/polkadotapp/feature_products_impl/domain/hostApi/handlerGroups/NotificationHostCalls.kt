@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class NotificationHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<PushNotificationParams, Int>("pushNotification") { params ->
            botApi.publishNotification(
                callingProductId = callingProductIdProvider.getProductId().getOrThrow(),
                text = params.text,
                deeplink = params.deeplink,
                scheduledAt = params.scheduledAtMs?.let(Instant::fromEpochMilliseconds),
            ).map { it.value }
        }

        bridge.registerHandler<Int, Unit>("cancelPushNotification") { identifier ->
            botApi.cancelNotification(
                callingProductId = callingProductIdProvider.getProductId().getOrThrow(),
                notificationId = NotificationId(identifier),
            )
        }
    }
}

private data class PushNotificationParams(
    val text: String,
    val deeplink: String?,
    val scheduledAtMs: Long?,
)
