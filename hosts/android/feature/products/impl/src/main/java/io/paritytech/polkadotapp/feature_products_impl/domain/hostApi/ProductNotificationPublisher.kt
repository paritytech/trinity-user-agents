package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import android.content.Context
import android.net.Uri
import androidx.core.app.NotificationCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import javax.inject.Inject

class ProductNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
) : NotificationPublisher(context, intentProvider) {
    fun publishNotification(notificationId: Int, text: String, deeplink: String?) {
        val channel = PolkadotNotificationChannel.PRODUCTS

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .setContentText(text)
            .setupDefaultNotification(deeplink?.let(Uri::parse))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .build()

        publish(notificationId, channel, notification)
    }
}
