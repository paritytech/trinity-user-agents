package io.paritytech.polkadotapp.feature_videogame_impl.data.notifications

import android.app.Notification
import android.content.Context
import android.media.RingtoneManager
import androidx.core.app.NotificationCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.VideoGameDeeplinkMapper
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class RealVideoGameNotificationPublisher @Inject constructor(
    @ApplicationContext context: Context,
    intentProvider: ActivityIntentProvider,
    private val videoGameDeeplinkMapper: VideoGameDeeplinkMapper
) : NotificationPublisher(context, intentProvider), VideoGameNotificationPublisher {
    private companion object {
        const val PRODUCT_GAME_NOTIFICATION_ID = 129
        const val PRODUCT_GAME_TAG_PREFIX = "product-game:"
    }

    // One notification per product, told apart by tag.
    override fun publishProductGameStartsSoonNotification(productId: ProductId, ringAlarm: Boolean) {
        val channel = if (ringAlarm) PolkadotNotificationChannel.VIDEO_GAME_ALARM else PolkadotNotificationChannel.VIDEO_GAME

        val builder = NotificationCompat.Builder(appContext, channel.id)
            .setupDefaultNotification(deepLink = videoGameDeeplinkMapper.toProductGameDeeplink(productId))
            .setContentTitle(appContext.getString(RCommon.string.video_game_start_reminder_title))
            .setContentText(appContext.getString(RCommon.string.video_game_start_reminder_message))
        val notification = if (ringAlarm) builder.buildAlarm(channel) else builder.build()

        publish(
            notificationId = PRODUCT_GAME_NOTIFICATION_ID,
            channel = channel,
            notification = notification,
            tag = productGameTag(productId),
        )
    }

    override fun cancelProductGameStartsSoonNotification(productId: ProductId) {
        cancel(PRODUCT_GAME_NOTIFICATION_ID, tag = productGameTag(productId))
    }

    private fun productGameTag(productId: ProductId) = PRODUCT_GAME_TAG_PREFIX + productId.value

    override fun cancelProductGameStartNotifications() {
        activeNotifications
            .filter { it.tag?.startsWith(PRODUCT_GAME_TAG_PREFIX) == true }
            .forEach { cancel(it.id, tag = it.tag) }
    }

    private fun NotificationCompat.Builder.buildAlarm(channel: PolkadotNotificationChannel): Notification =
        setCategory(NotificationCompat.CATEGORY_ALARM)
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setVibrate(channel.vibrationPattern)
            .setSound(RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE))
            .build()
            .applyAlarmFlags()

    private fun Notification.applyAlarmFlags(): Notification {
        flags = flags or Notification.FLAG_INSISTENT
        return this
    }
}
