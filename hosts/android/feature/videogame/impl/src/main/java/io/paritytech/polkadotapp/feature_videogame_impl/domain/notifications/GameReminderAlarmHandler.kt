package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import uniffi.truapi.ProductId
import javax.inject.Inject

class GameReminderAlarmHandler @Inject constructor(
    private val notificationPublisher: VideoGameNotificationPublisher,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val productGameReminder: RealProductGameReminder,
    private val telemetry: GameReminderTelemetry,
) {
    fun onAlarm(productId: ProductId, ringAlarm: Boolean) {
        val foreground = appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND
        val game = productGameReminder.scheduledFor(productId)
        telemetry.report(
            GameReminderEvent.Fired(
                productId,
                game?.startsAtMillis,
                foreground = foreground,
                stillScheduled = game != null,
                delivery = if (ringAlarm) GameReminderDelivery.ALARM else GameReminderDelivery.NOTIFICATION,
            )
        )
        // In the foreground the pill and the auto-open take over.
        // An inexact alarm may fire late; a game still scheduled wants its notification then.
        if (foreground || game == null) return
        notificationPublisher.publishProductGameStartsSoonNotification(game.product(), ringAlarm = ringAlarm)
    }
}
