package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import android.content.Context
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.os.bundleOf
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_MARKER
import io.paritytech.polkadotapp.common.data.telemetry.PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID
import io.paritytech.polkadotapp.common.data.telemetry.productNotificationShown
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ProductId
import javax.inject.Inject

class ProductNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
    private val originProvider: HostEventOriginProvider,
    private val capture: HostEventCapture,
) : NotificationPublisher(context, intentProvider) {
    fun publishNotification(notificationId: Int, productId: ProductId, text: String, deeplink: String?) {
        val channel = PolkadotNotificationChannel.PRODUCTS
        val contentIntentExtras = bundleOf(
            PRODUCT_NOTIFICATION_INTENT_MARKER to true,
            PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID to productId.value,
        )

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .setContentText(text)
            .setupDefaultNotification(
                deepLink = deeplink?.let(Uri::parse),
                contentIntentExtras = contentIntentExtras,
                // Own PendingIntent per notification, so a tap carries this notification's extras.
                contentIntentRequestCode = notificationId,
            )
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .build()

        if (publish(notificationId, channel, notification)) {
            runCatching {
                capture.capture {
                    originProvider.current().productNotificationShown(productId.value)
                }
            }.logFailure("Failed to capture product notification shown event")
        }
    }
}
