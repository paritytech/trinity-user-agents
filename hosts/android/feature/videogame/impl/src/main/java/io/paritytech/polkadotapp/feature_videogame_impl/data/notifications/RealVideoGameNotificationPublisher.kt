package io.paritytech.polkadotapp.feature_videogame_impl.data.notifications

import android.app.Notification
import android.content.Context
import android.media.RingtoneManager
import android.net.Uri
import androidx.core.app.NotificationCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
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
    private val timeFormatter: TimeFormatter,
    private val videoGameDeeplinkMapper: VideoGameDeeplinkMapper
) : NotificationPublisher(context, intentProvider), VideoGameNotificationPublisher {
    private companion object {
        const val REGISTERED_GAME_NOTIFICATION_ID = 128
        const val PRODUCT_GAME_NOTIFICATION_ID = 129
        const val PRODUCT_GAME_TAG_PREFIX = "product-game:"
    }

    override fun publishRegistrationOpenedNotification(timestamp: Timestamp) {
        val channel = PolkadotNotificationChannel.VIDEO_GAME
        val deeplink = videoGameDeeplinkMapper.toWeeklyGameBotDeeplink()

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .setupDefaultNotification(deepLink = deeplink)
            .setContentTitle(
                appContext.getString(
                    RCommon.string.video_game_registration_reminder_title,
                    timeFormatter.formatMonth(timestamp, false)
                )
            )
            .setContentText(appContext.getString(RCommon.string.video_game_registration_reminder_message))
            .build()

        publish(timestamp.hashCode(), channel, notification)
    }

    override fun publishWaitingRoomAvailableNotification() {
        val channel = PolkadotNotificationChannel.VIDEO_GAME
        val deeplink = videoGameDeeplinkMapper.toWeeklyGameBotDeeplink()

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .setupDefaultNotification(deepLink = deeplink)
            .setContentTitle(appContext.getString(RCommon.string.video_game_waiting_room_reminder_title))
            .setContentText(appContext.getString(RCommon.string.video_game_waiting_room_reminder_message))
            .build()

        publish(REGISTERED_GAME_NOTIFICATION_ID, channel, notification)
    }

    override fun publishGameAboutToStartNotification() {
        val channel = PolkadotNotificationChannel.VIDEO_GAME
        val deeplink = videoGameDeeplinkMapper.toWaitingRoomDeeplink()

        val notification = NotificationCompat.Builder(appContext, channel.id)
            .setupDefaultNotification(deepLink = deeplink)
            .setContentTitle(appContext.getString(RCommon.string.video_game_about_to_start_reminder_title))
            .setContentText(appContext.getString(RCommon.string.video_game_about_to_start_reminder_message))
            .build()

        publish(REGISTERED_GAME_NOTIFICATION_ID, channel, notification)
    }

    override fun publishGameStartsSoonNotification() {
        publishGameStartsSoon(videoGameDeeplinkMapper.toWaitingRoomDeeplink(), REGISTERED_GAME_NOTIFICATION_ID, ringAlarm = true)
    }

    // One notification per product, told apart by tag.
    override fun publishProductGameStartsSoonNotification(productId: ProductId, ringAlarm: Boolean) {
        publishGameStartsSoon(
            videoGameDeeplinkMapper.toProductGameDeeplink(productId),
            PRODUCT_GAME_NOTIFICATION_ID,
            ringAlarm,
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

    override fun cancelGameStartNotifications() {
        cancel(REGISTERED_GAME_NOTIFICATION_ID)
    }

    private fun publishGameStartsSoon(deeplink: Uri, notificationId: Int, ringAlarm: Boolean, tag: String? = null) {
        val channel = if (ringAlarm) PolkadotNotificationChannel.VIDEO_GAME_ALARM else PolkadotNotificationChannel.VIDEO_GAME

        val builder = NotificationCompat.Builder(appContext, channel.id)
            .setupDefaultNotification(deepLink = deeplink)
            .setContentTitle(appContext.getString(RCommon.string.video_game_start_reminder_title))
            .setContentText(appContext.getString(RCommon.string.video_game_start_reminder_message))
        val notification = if (ringAlarm) builder.buildAlarm(channel) else builder.build()

        publish(
            notificationId = notificationId,
            channel = channel,
            notification = notification,
            tag = tag,
        )
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
