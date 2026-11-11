@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import android.content.Context
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.os.bundleOf
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_MARKER
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_NOTIFICATION_ID
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationTelemetry
import uniffi.truapi.ProductId
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class ProductNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
    private val telemetry: ProductNotificationTelemetry,
) : NotificationPublisher(context, intentProvider) {
    fun publishNotification(
        notificationId: NotificationId,
        productId: ProductId,
        text: String,
        deeplink: String?,
        scheduledAt: Instant?,
    ) {
        val channel = PolkadotNotificationChannel.PRODUCTS
        val contentIntentExtras = bundleOf(
            PRODUCT_NOTIFICATION_INTENT_MARKER to true,
            PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID to productId.value,
            PRODUCT_NOTIFICATION_INTENT_NOTIFICATION_ID to notificationId.value,
        )

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .setContentText(text)
            .setupDefaultNotification(
                deepLink = deeplink?.let(Uri::parse),
                contentIntentExtras = contentIntentExtras,
                // Own PendingIntent per notification, so a tap carries this notification's extras.
                contentIntentRequestCode = notificationId.value,
            )
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .build()

        if (publish(notificationId.value, channel, notification)) {
            telemetry.shown(productId, notificationId, scheduledAt)
        } else {
            val reason = suppressedReason(NotificationManagerCompat.from(appContext).areNotificationsEnabled())
            telemetry.suppressed(productId, notificationId, scheduledAt, reason)
        }
    }
}

/**
 * Why a notification the host tried to post did not reach the person.
 *
 * With notifications on for the app, the only other way to fail is a blocked channel.
 */
internal fun suppressedReason(notificationsEnabled: Boolean): ProductNotificationReason =
    if (notificationsEnabled) {
        ProductNotificationReason.CHANNEL_BLOCKED
    } else {
        ProductNotificationReason.NOTIFICATIONS_DISABLED
    }
