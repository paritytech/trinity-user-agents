package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderDelivery
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderTelemetry
import javax.inject.Inject

// What VideoGameReminderBroadcastReceiver does once it has read its intent.
class GameReminderAlarmHandler @Inject constructor(
    private val notificationPublisher: VideoGameNotificationPublisher,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val productGameReminder: RealProductGameReminder,
    private val telemetry: GameReminderTelemetry,
) {
    // In the foreground the pill and the auto-open take over. An inexact alarm may fire late; a game still
    // scheduled wants its notification then.
    fun onAlarm(productId: ProductId, startsAtMillis: Long?, ringAlarm: Boolean) {
        val foreground = appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND
        val game = productGameReminder.scheduledFor(productId)
        telemetry.report(
            GameReminderEvent.Fired(
                productId,
                startsAtMillis ?: game?.startsAtMillis,
                foreground = foreground,
                stillScheduled = game != null,
                delivery = if (ringAlarm) GameReminderDelivery.ALARM else GameReminderDelivery.NOTIFICATION,
            )
        )
        if (foreground || game == null) return
        notificationPublisher.publishProductGameStartsSoonNotification(game.product(), game.startsAtMillis, ringAlarm = ringAlarm)
    }

    fun onDismissed(productId: ProductId, startsAtMillis: Long?) {
        telemetry.report(GameReminderEvent.Dismissed(productId, startsAtMillis))
    }
}
