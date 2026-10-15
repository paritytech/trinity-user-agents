package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import android.content.Context
import android.net.Uri
import androidx.core.app.NotificationCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.MutedProductRoomCheck
import javax.inject.Inject

class ProductNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
    private val mutedRoomCheck: MutedProductRoomCheck,
) : NotificationPublisher(context, intentProvider) {
    /** Shows the notification of [productId], unless it opens a room of the product that the user muted (D34). */
    suspend fun publishProductNotification(productId: ProductId, notificationId: Int, text: String, deeplink: String?) {
        if (mutedRoomCheck.isMuted(productId, deeplink)) return

        publishNotification(notificationId, text, deeplink)
    }

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
