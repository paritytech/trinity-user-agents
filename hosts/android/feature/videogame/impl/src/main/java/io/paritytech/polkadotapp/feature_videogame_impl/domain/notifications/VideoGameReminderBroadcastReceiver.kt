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
import io.paritytech.polkadotapp.feature_videogame_impl.service.VideoGameStateReader
import io.paritytech.polkadotapp.feature_videogame_impl.service.isInWaitingRoom
import javax.inject.Inject

@AndroidEntryPoint
class VideoGameReminderBroadcastReceiver : BroadcastReceiver() {
    companion object {
        const val ACTION_POST_NOTIFICATION = "io.paritytech.polkadotapp.feature_videogame.domain.notifications.POST_NOTIFICATION"
        const val EXTRA_NOTIFICATION_TYPE = "notification_type"
    }

    @Inject
    lateinit var notificationPublisher: VideoGameNotificationPublisher

    @Inject
    lateinit var appLifecycleObserver: AppLifecycleObserver

    @Inject
    lateinit var videoGameStateReader: VideoGameStateReader

    @Inject
    lateinit var productGameReminder: RealProductGameReminder

    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            ACTION_POST_NOTIFICATION -> {
                when (val type = intent.getParcelableExtra<VideoGameNotificationType>(EXTRA_NOTIFICATION_TYPE)) {
                    is VideoGameNotificationType.RegistrationOpened -> {
                        notificationPublisher.publishRegistrationOpenedNotification(type.timestamp)
                    }

                    VideoGameNotificationType.WaitingRoomAvailable -> {
                        notificationPublisher.publishWaitingRoomAvailableNotification()
                    }

                    VideoGameNotificationType.GameAboutToStart -> {
                        notificationPublisher.publishGameAboutToStartNotification()
                    }

                    VideoGameNotificationType.GameStartsSoon -> {
                        val isAppInForeground = appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND
                        val isViewingWaitingRoom = isAppInForeground && videoGameStateReader.isInWaitingRoom()
                        if (!isViewingWaitingRoom) {
                            notificationPublisher.publishGameStartsSoonNotification()
                        }
                    }

                    is VideoGameNotificationType.ProductGameStartsSoon -> {
                        publishProductGameStartsSoon(context, ProductId.fromStoredValue(type.productId))
                    }

                    null -> Unit
                }
            }
        }
    }

    // In the foreground the pill and the auto-open take over. An inexact alarm may fire late; a game still
    // scheduled wants its notification then.
    private fun publishProductGameStartsSoon(context: Context, productId: ProductId) {
        if (appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND) return
        val game = productGameReminder.scheduledFor(productId) ?: return
        notificationPublisher.publishProductGameStartsSoonNotification(game.product(), ringAlarm = context.canScheduleExactAlarms())
    }
}
