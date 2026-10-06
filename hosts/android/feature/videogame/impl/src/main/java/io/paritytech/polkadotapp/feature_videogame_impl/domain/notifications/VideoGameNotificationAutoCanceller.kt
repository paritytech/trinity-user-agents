package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.service.VideoGameStateReader
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject

/**
 * Cancels game-start notifications when the app enters the foreground or when the user
 * reaches an in-game screen (signaled by [VideoGameStateReader.gameSnapshot] becoming non-null).
 */
class VideoGameNotificationAutoCanceller @Inject constructor(
    private val notificationPublisher: VideoGameNotificationPublisher,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val videoGameStateReader: VideoGameStateReader
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        if (FeatureOption.PERSONHOOD.isDisabled) return@runCancellableCatching

        // Cancel when the app comes to the foreground.
        appLifecycleObserver.subscribe()
            .filter { it == AppLifecycleState.FOREGROUND }
            .onEach { notificationPublisher.cancelGameStartNotifications() }
            .launchIn(scope)

        // Cancel once when the user enters a game session (null -> non-null transition only,
        // so subsequent in-game state updates don't re-cancel later notifications).
        videoGameStateReader.gameSnapshot
            .map { it != null }
            .distinctUntilChanged()
            .filter { it }
            .onEach { notificationPublisher.cancelGameStartNotifications() }
            .launchIn(scope)
    }
}
