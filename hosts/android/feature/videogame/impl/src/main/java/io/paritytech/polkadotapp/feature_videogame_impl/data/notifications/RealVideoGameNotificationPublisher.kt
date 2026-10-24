package io.paritytech.polkadotapp.feature_videogame_impl.data.notifications

import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.media.RingtoneManager
import androidx.core.app.NotificationCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.notifications.NotificationPublisher
import io.paritytech.polkadotapp.common.presentation.notifications.PolkadotNotificationChannel
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.VideoGameDeeplinkMapper
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.VideoGameNotificationType
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.VideoGameReminderBroadcastReceiver
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
        const val PRODUCT_GAME_DISMISS_REQUEST_CODE_MASK = 0x3FFFFFFF
    }

    // One notification per product, told apart by tag.
    override fun publishProductGameStartsSoonNotification(productId: ProductId, startsAtMillis: Long, ringAlarm: Boolean) {
        val channel = if (ringAlarm) PolkadotNotificationChannel.VIDEO_GAME_ALARM else PolkadotNotificationChannel.VIDEO_GAME

        val builder = NotificationCompat.Builder(appContext, channel.id)
            .setupDefaultNotification(deepLink = videoGameDeeplinkMapper.toProductGameDeeplink(productId, startsAtMillis))
            .setDeleteIntent(dismissIntent(productId, startsAtMillis))
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

    // Fires on a swipe or clear-all only; neither a tap nor a cancel from code sends it.
    private fun dismissIntent(productId: ProductId, startsAtMillis: Long): PendingIntent {
        val intent = Intent(appContext, VideoGameReminderBroadcastReceiver::class.java).apply {
            action = VideoGameReminderBroadcastReceiver.ACTION_NOTIFICATION_DISMISSED
            putExtra(
                VideoGameReminderBroadcastReceiver.EXTRA_NOTIFICATION_TYPE,
                VideoGameNotificationType.ProductGameStartsSoon(productId.value),
            )
            putExtra(VideoGameReminderBroadcastReceiver.EXTRA_GAME_STARTS_AT, startsAtMillis)
        }
        return PendingIntent.getBroadcast(
            appContext,
            productGameTag(productId).hashCode() and PRODUCT_GAME_DISMISS_REQUEST_CODE_MASK,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

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
