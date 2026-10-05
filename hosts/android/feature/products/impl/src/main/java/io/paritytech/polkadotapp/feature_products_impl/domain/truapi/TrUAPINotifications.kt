package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductNotificationPublisher
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationReminderBroadcastReceiver
import uniffi.truapi.HostPushNotificationRequest
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TrUAPINotifications @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val publisher: ProductNotificationPublisher,
) {
    private val alarms = context.getSystemService(AlarmManager::class.java)

    fun schedule(walletId: String, productId: String, id: UInt, request: HostPushNotificationRequest) {
        val scheduledAt = request.scheduledAt
        if (scheduledAt == null || scheduledAt.toLong() <= System.currentTimeMillis()) {
            publisher.publishNotification(id.toInt(), request.text, request.deeplink, intent(walletId, productId, id).dataString)
            return
        }
        val intent = intent(walletId, productId, id)
            .putExtra(ProductNotificationReminderBroadcastReceiver.EXTRA_TRUAPI_TEXT, request.text)
            .putExtra(ProductNotificationReminderBroadcastReceiver.EXTRA_TRUAPI_DEEPLINK, request.deeplink)
        alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, scheduledAt.toLong(), pending(id, intent))
    }

    fun isPending(walletId: String, productId: String, id: UInt): Boolean =
        PendingIntent.getBroadcast(context, id.toInt(), intent(walletId, productId, id), PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE) != null

    fun cancel(walletId: String, productId: String, id: UInt) {
        val pending = pending(id, intent(walletId, productId, id))
        alarms.cancel(pending)
        pending.cancel()
    }

    private fun intent(walletId: String, productId: String, id: UInt) =
        Intent(context, ProductNotificationReminderBroadcastReceiver::class.java).apply {
            action = ProductNotificationReminderBroadcastReceiver.ACTION_POST_PRODUCT_NOTIFICATION
            data = android.net.Uri.parse("truapi-notification://${android.net.Uri.encode(walletId)}/${android.net.Uri.encode(productId)}/$id")
            putExtra(ProductNotificationReminderBroadcastReceiver.EXTRA_PRODUCT_ID, productId)
            putExtra(ProductNotificationReminderBroadcastReceiver.EXTRA_NOTIFICATION_ID, id.toInt())
        }

    private fun pending(id: UInt, intent: Intent): PendingIntent = PendingIntent.getBroadcast(
        context, id.toInt(), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
}
