@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import io.paritytech.polkadotapp.common.utils.launchAsyncJob
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductNotificationPublisher
import uniffi.truapi.ProductId
import javax.inject.Inject
import kotlin.time.ExperimentalTime

@AndroidEntryPoint
class ProductNotificationReminderBroadcastReceiver : BroadcastReceiver() {
    companion object {
        const val ACTION_POST_PRODUCT_NOTIFICATION =
            "io.paritytech.polkadotapp.feature_products.domain.notifications.POST_PRODUCT_NOTIFICATION"
        const val EXTRA_PRODUCT_ID = "product_id"
        const val EXTRA_NOTIFICATION_ID = "notification_id"
    }

    @Inject
    lateinit var notificationPublisher: ProductNotificationPublisher

    @Inject
    lateinit var repository: ScheduledProductNotificationRepository

    @Inject
    lateinit var telemetry: ProductNotificationTelemetry

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_POST_PRODUCT_NOTIFICATION) return

        val productIdValue = intent.getStringExtra(EXTRA_PRODUCT_ID) ?: return
        if (!intent.hasExtra(EXTRA_NOTIFICATION_ID)) return
        val notificationIdValue = intent.getIntExtra(EXTRA_NOTIFICATION_ID, 0)

        val productId = ProductId.parse(productIdValue).getOrNull() ?: return
        val notificationId = NotificationId(notificationIdValue)

        launchAsyncJob {
            val row = repository.find(productId, notificationId)
            if (row == null) {
                telemetry.suppressed(
                    productId,
                    notificationId,
                    scheduledAt = null,
                    ProductNotificationReason.BOOKING_MISSING,
                )
                return@launchAsyncJob
            }
            notificationPublisher.publishNotification(
                notificationId,
                productId,
                row.text,
                row.deeplink,
                row.scheduledAt,
            )
            repository.delete(productId, notificationId)
        }
    }
}
