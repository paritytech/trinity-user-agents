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
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationPresentation
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductRoomNotificationPolicy
import javax.inject.Inject

class ProductNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
    private val mutedRoomCheck: MutedProductRoomCheck,
    private val roomPolicy: ProductRoomNotificationPolicy,
) : NotificationPublisher(context, intentProvider) {
    /**
     * Shows the notification of [productId], unless it opens a room of the product that the user muted (D34) or that
     * is on screen. A notification that opens a room of the product has the room name as its title.
     */
    suspend fun publishProductNotification(productId: ProductId, notificationId: Int, text: String, deeplink: String?) {
        if (mutedRoomCheck.isMuted(productId, deeplink)) return

        when (val presentation = roomPolicy.presentation(productId, deeplink)) {
            ProductNotificationPresentation.Hidden -> return
            is ProductNotificationPresentation.Shown -> publishNotification(notificationId, text, deeplink, presentation.title)
        }
    }

    fun publishNotification(notificationId: Int, text: String, deeplink: String?, title: String? = null) {
        val channel = PolkadotNotificationChannel.PRODUCTS

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .apply { title?.let(::setContentTitle) }
            .setContentText(text)
            .setupDefaultNotification(deeplink?.let(Uri::parse))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .build()

        publish(notificationId, channel, notification)
    }
}
