@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import android.content.Context
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.os.bundleOf
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_MARKER
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_NOTIFICATION_ID
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.common.presentation.notifications.PublishOutcome
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

        val suppressedReason = publish(notificationId.value, channel, notification).suppressedReason
        if (suppressedReason == null) {
            telemetry.shown(productId, notificationId, scheduledAt)
        } else {
            telemetry.suppressed(productId, notificationId, scheduledAt, suppressedReason)
        }
    }
}

internal val PublishOutcome.suppressedReason: ProductNotificationReason?
    get() = when (this) {
        PublishOutcome.POSTED -> null
        PublishOutcome.NOTIFICATIONS_DISABLED -> ProductNotificationReason.NOTIFICATIONS_DISABLED
        PublishOutcome.CHANNEL_BLOCKED -> ProductNotificationReason.CHANNEL_BLOCKED
    }
