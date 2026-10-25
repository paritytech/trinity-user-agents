package io.paritytech.polkadotapp.feature_videogame_impl.presentation.autoLaunch

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.subscribeIsForeground
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderOpenSource
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderTelemetry
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealProductGameReminder
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ScheduledProductGame
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.isLiveAt
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.product
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.mapLatest
import javax.inject.Inject

@OptIn(ExperimentalCoroutinesApi::class)
class ProductGameAutoOpener @Inject constructor(
    private val reminder: RealProductGameReminder,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val router: VideoGameRouter,
    private val timeProvider: TimeProvider,
    private val telemetry: GameReminderTelemetry,
) : AppInitializer {
    private val pendingGames = mutableSetOf<ScheduledProductGame>()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        combine(reminder.scheduled, appLifecycleObserver.subscribeIsForeground()) { games, foreground -> games to foreground }
            .mapLatest { (games, foreground) ->
                if (!foreground) return@mapLatest
                reportMissed(games)
                nextLive(games)?.let { openAtStart(it) }
            }
            .launchIn(scope)
    }

    private fun reportMissed(games: List<ScheduledProductGame>) {
        val now = timeProvider.now().toEpochMilliseconds()
        pendingGames.retainAll(games.toSet())
        games.filterNot { it.isLiveAt(now) }.filter { pendingGames.remove(it) }.forEach { game ->
            telemetry.report(GameReminderEvent.Missed(game.product(), game.startsAtMillis))
        }
        pendingGames += games.filter { it.isLiveAt(now) }
    }

    private fun nextLive(games: List<ScheduledProductGame>): ScheduledProductGame? {
        val now = timeProvider.now().toEpochMilliseconds()
        return games.filter { it.isLiveAt(now) }.minByOrNull { it.startsAtMillis }
    }

    private suspend fun openAtStart(game: ScheduledProductGame) {
        delay(game.startsAtMillis - timeProvider.now().toEpochMilliseconds())
        router.openGameProduct(game.product())
        telemetry.report(GameReminderEvent.ProductOpened(game.product(), game.startsAtMillis, GameReminderOpenSource.AUTO_OPEN))
        reminder.clear(game)
    }
}
