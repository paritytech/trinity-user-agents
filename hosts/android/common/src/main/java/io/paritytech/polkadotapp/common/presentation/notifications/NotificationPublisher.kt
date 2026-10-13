package io.paritytech.polkadotapp.common.presentation.notifications

import android.annotation.SuppressLint
import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.net.Uri
import android.service.notification.StatusBarNotification
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.R as RCommon

abstract class NotificationPublisher(
    protected val appContext: Context,
    protected val intentProvider: ActivityIntentProvider
) {
    private val notificationManager = NotificationManagerCompat.from(appContext)

    protected val activeNotifications: List<StatusBarNotification>
        get() = notificationManager.activeNotifications.toList()

    @SuppressLint("MissingPermission")
    protected fun publish(
        notificationId: Int,
        channel: PolkadotNotificationChannel,
        notification: Notification,
        tag: String? = null,
    ) {
        if (notificationManager.areNotificationsEnabled().not()) return

        createChannel(channel)

        notificationManager.notify(tag, notificationId, notification)
    }

    private fun createChannel(channel: PolkadotNotificationChannel) {
        // Android won't update channel behavior after initial creation.
        // Delete the old channel to force recreation with correct settings for existing users.
        deleteChannelIfOutdated(channel.id, channel.vibrationPattern)

        val notificationChannel = NotificationChannelCompat.Builder(channel.id, channel.importance)
            .setName(appContext.getString(channel.nameRes))
            .setDescription(appContext.getString(channel.descriptionRes))
            .apply {
                if (channel.vibrationPattern != null) {
                    setVibrationEnabled(true)
                    setVibrationPattern(channel.vibrationPattern)
                }
            }
            .build()

        notificationManager.createNotificationChannel(notificationChannel)
    }

    private fun deleteChannelIfOutdated(channelId: String, vibrationPattern: LongArray?) {
        val existingChannel = notificationManager.getNotificationChannelCompat(channelId) ?: return
        if (!existingChannel.vibrationPattern.contentEquals(vibrationPattern)) {
            notificationManager.deleteNotificationChannel(channelId)
        }
    }

    protected fun NotificationCompat.Builder.setupDefaultNotification(deepLink: Uri? = null): NotificationCompat.Builder {
        return this
            .setSmallIcon(RCommon.drawable.ic_notification_default)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setContentIntent(createPendingIntent(deepLink))
            .setAutoCancel(true)
    }

    private fun createPendingIntent(deepLink: Uri?): PendingIntent {
        val intent = intentProvider.getRootIntent().apply {
            if (deepLink != null) {
                data = deepLink
            }
        }

        return PendingIntent.getActivity(
            appContext,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    protected fun getNotification(notificationId: Int): Notification? {
        return notificationManager.activeNotifications
            .find { it.id == notificationId }
            ?.notification
    }

    protected fun cancel(notificationId: Int, tag: String? = null) {
        notificationManager.cancel(tag, notificationId)
    }
}
