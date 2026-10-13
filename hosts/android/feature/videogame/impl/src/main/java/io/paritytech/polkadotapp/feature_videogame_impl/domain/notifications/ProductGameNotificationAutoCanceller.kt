package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject

// In the foreground the pill and the auto-open take over from the notification.
class ProductGameNotificationAutoCanceller @Inject constructor(
    private val notificationPublisher: VideoGameNotificationPublisher,
    private val appLifecycleObserver: AppLifecycleObserver,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        appLifecycleObserver.subscribe()
            .filter { it == AppLifecycleState.FOREGROUND }
            .onEach { notificationPublisher.cancelProductGameStartNotifications() }
            .launchIn(scope)
    }
}
