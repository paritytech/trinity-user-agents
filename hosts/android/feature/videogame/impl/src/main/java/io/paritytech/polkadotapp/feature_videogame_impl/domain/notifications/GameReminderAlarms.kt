package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.annotation.SuppressLint
import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Intent
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.canScheduleExactAlarms
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

// The AlarmManager side of the reminder, apart so the scheduler's decisions run in unit tests.
interface GameReminderAlarms {
    fun canRingExactly(): Boolean
    fun set(productId: ProductId, atMillis: Timestamp, exact: Boolean)
    fun cancel(productId: ProductId)
}

class RealGameReminderAlarms @Inject constructor(
    private val contextManager: ContextManager,
) : GameReminderAlarms {
    private companion object {
        const val PRODUCT_GAME_REQUEST_CODE_BASE = 1000
        const val PRODUCT_GAME_REQUEST_CODE_MASK = 0x3FFFFFFF
    }

    private val alarmManager = contextManager.applicationContext.getSystemService(AlarmManager::class.java)

    override fun canRingExactly(): Boolean = contextManager.applicationContext.canScheduleExactAlarms()

    @SuppressLint("MissingPermission")
    override fun set(productId: ProductId, atMillis: Timestamp, exact: Boolean) {
        val pendingIntent = createPendingIntent(productId, atMillis)

        alarmManager.cancel(pendingIntent)

        if (exact) {
            alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMillis, pendingIntent)
        } else {
            alarmManager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, atMillis, pendingIntent)
        }
    }

    override fun cancel(productId: ProductId) {
        alarmManager.cancel(createPendingIntent(productId, gameStartMillis = null))
    }

    private fun createPendingIntent(productId: ProductId, gameStartMillis: Timestamp?): PendingIntent {
        val notificationType = VideoGameNotificationType.ProductGameStartsSoon(productId.value)
        val intent = Intent(contextManager.applicationContext, VideoGameReminderBroadcastReceiver::class.java).apply {
            action = VideoGameReminderBroadcastReceiver.ACTION_POST_NOTIFICATION
            putExtra(VideoGameReminderBroadcastReceiver.EXTRA_NOTIFICATION_TYPE, notificationType)
            gameStartMillis?.let { putExtra(VideoGameReminderBroadcastReceiver.EXTRA_GAME_STARTS_AT, it) }
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
