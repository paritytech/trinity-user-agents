package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.utils.canScheduleExactAlarms
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

@AndroidEntryPoint
class VideoGameReminderBroadcastReceiver : BroadcastReceiver() {
    companion object {
        const val ACTION_POST_NOTIFICATION = "io.paritytech.polkadotapp.feature_videogame.domain.notifications.POST_NOTIFICATION"
        const val EXTRA_NOTIFICATION_TYPE = "notification_type"
    }

    @Inject
    lateinit var alarmHandler: GameReminderAlarmHandler

    override fun onReceive(context: Context, intent: Intent) {
        val type = intent.getParcelableExtra<VideoGameNotificationType>(EXTRA_NOTIFICATION_TYPE)
        if (type !is VideoGameNotificationType.ProductGameStartsSoon) return
        val productId = ProductId.fromStoredValue(type.productId)
        if (intent.action == ACTION_POST_NOTIFICATION) {
            alarmHandler.onAlarm(productId, ringAlarm = context.canScheduleExactAlarms())
        }
    }
}
