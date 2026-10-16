package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.annotation.SuppressLint
import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.VideoGameSettingsPreferences
import javax.inject.Inject

interface VideoGameReminderScheduler {
    fun scheduleProductGameStart(productId: ProductId, gameStartMillis: Timestamp)
    fun cancelProductGameStart(productId: ProductId)
}

class RealVideoGameReminderScheduler @Inject constructor(
    private val contextManager: ContextManager,
    private val alarmPreferences: VideoGameSettingsPreferences,
) : VideoGameReminderScheduler {
    private companion object {
        const val PRODUCT_GAME_REQUEST_CODE_BASE = 1000
        const val PRODUCT_GAME_REQUEST_CODE_MASK = 0x3FFFFFFF
    }

    private val alarmManager = contextManager.applicationContext.getSystemService(AlarmManager::class.java)

    override fun scheduleProductGameStart(productId: ProductId, gameStartMillis: Timestamp) {
        val notificationType = VideoGameNotificationType.ProductGameStartsSoon(productId.value)
        val triggerAtMillis = gameStartMillis - alarmPreferences.getAlarmOffset().inWholeMilliseconds

        if (triggerAtMillis <= System.currentTimeMillis()) {
            cancelAlarm(notificationType)
            return
        }

        scheduleAlarm(notificationType, triggerAtMillis)
    }

    override fun cancelProductGameStart(productId: ProductId) =
        cancelAlarm(VideoGameNotificationType.ProductGameStartsSoon(productId.value))

    private fun cancelAlarm(notificationType: VideoGameNotificationType) {
        alarmManager.cancel(createPendingIntent(notificationType))
    }

    @SuppressLint("MissingPermission")
    private fun scheduleAlarm(notificationType: VideoGameNotificationType, timestamp: Timestamp) {
        val pendingIntent = createPendingIntent(notificationType)

        alarmManager.cancel(pendingIntent)

        val canScheduleAlarms = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            alarmManager.canScheduleExactAlarms()
        } else true

        if (canScheduleAlarms) {
            alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, timestamp, pendingIntent)
        } else {
            alarmManager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, timestamp, pendingIntent)
        }
    }

    private fun createPendingIntent(
        notificationType: VideoGameNotificationType,
    ): PendingIntent {
        val intent = Intent(contextManager.applicationContext, VideoGameReminderBroadcastReceiver::class.java).apply {
            action = VideoGameReminderBroadcastReceiver.ACTION_POST_NOTIFICATION
            putExtra(VideoGameReminderBroadcastReceiver.EXTRA_NOTIFICATION_TYPE, notificationType)
        }

        return PendingIntent.getBroadcast(
            contextManager.applicationContext,
            notificationType.requestCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    // One pending intent per product. The hash is masked non-negative and small enough not to overflow.
    private fun VideoGameNotificationType.requestCode(): Int = when (this) {
        is VideoGameNotificationType.ProductGameStartsSoon ->
            PRODUCT_GAME_REQUEST_CODE_BASE + (productId.hashCode() and PRODUCT_GAME_REQUEST_CODE_MASK)
    }
}
