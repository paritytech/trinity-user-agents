package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.ServiceInfo
import androidx.annotation.StringRes
import androidx.core.app.NotificationCompat
import androidx.work.ForegroundInfo
import io.paritytech.polkadotapp.common.R as RCommon

/** The notification of a file transfer that runs in the foreground (an upload of the outbox of a room of the chat product). */
internal fun hopTransferForegroundInfo(
    context: Context,
    notificationId: Int,
    @StringRes titleRes: Int,
    @StringRes messageRes: Int
): ForegroundInfo {
    val channelId = context.getString(RCommon.string.workers_notification_channel_id)
    val notificationManager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    notificationManager.createNotificationChannel(
        NotificationChannel(
            channelId,
            context.getString(RCommon.string.workers_notification_channel_name),
            NotificationManager.IMPORTANCE_LOW
        )
    )

    val notification = NotificationCompat.Builder(context, channelId)
        .setContentTitle(context.getString(titleRes))
        .setContentText(context.getString(messageRes))
        .setSmallIcon(RCommon.drawable.ic_notification_default)
        .setOngoing(true)
        .build()

    return ForegroundInfo(notificationId, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
}
