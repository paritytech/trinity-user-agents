package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.canScheduleExactAlarms
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderTelemetry
import javax.inject.Inject

@AndroidEntryPoint
class VideoGameReminderBroadcastReceiver : BroadcastReceiver() {
    companion object {
        const val ACTION_POST_NOTIFICATION = "io.paritytech.polkadotapp.feature_videogame.domain.notifications.POST_NOTIFICATION"
        const val ACTION_NOTIFICATION_DISMISSED = "io.paritytech.polkadotapp.feature_videogame.domain.notifications.NOTIFICATION_DISMISSED"
        const val EXTRA_NOTIFICATION_TYPE = "notification_type"
        const val EXTRA_GAME_STARTS_AT = "game_starts_at"
    }

    @Inject
    lateinit var notificationPublisher: VideoGameNotificationPublisher

    @Inject
    lateinit var appLifecycleObserver: AppLifecycleObserver

    @Inject
    lateinit var productGameReminder: RealProductGameReminder

    @Inject
    lateinit var telemetry: GameReminderTelemetry

    override fun onReceive(context: Context, intent: Intent) {
        val type = intent.getParcelableExtra<VideoGameNotificationType>(EXTRA_NOTIFICATION_TYPE)
        if (type !is VideoGameNotificationType.ProductGameStartsSoon) return
        val productId = ProductId.fromStoredValue(type.productId)
        val startsAtMillis = intent.getLongExtra(EXTRA_GAME_STARTS_AT, -1).takeIf { it >= 0 }

        when (intent.action) {
            ACTION_POST_NOTIFICATION -> publishProductGameStartsSoon(context, productId, startsAtMillis)
            ACTION_NOTIFICATION_DISMISSED -> telemetry.report(GameReminderEvent.Dismissed(productId, startsAtMillis))
        }
    }

    // In the foreground the pill and the auto-open take over. An inexact alarm may fire late; a game still
    // scheduled wants its notification then.
    private fun publishProductGameStartsSoon(context: Context, productId: ProductId, startsAtMillis: Long?) {
        val foreground = appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND
        val game = productGameReminder.scheduledFor(productId)
        telemetry.report(
            GameReminderEvent.Fired(
                productId,
                startsAtMillis ?: game?.startsAtMillis,
                foreground = foreground,
                stillScheduled = game != null,
            )
        )
        if (foreground || game == null) return
        notificationPublisher.publishProductGameStartsSoonNotification(
            game.product(),
            game.startsAtMillis,
            ringAlarm = context.canScheduleExactAlarms(),
        )
    }
}
