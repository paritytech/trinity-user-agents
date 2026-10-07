package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.currentTimestampFlow
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealProductGameReminder
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ScheduledProductGame
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.product
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes

private val PILL_SHOWN_BEFORE_START = 5.minutes

@HiltViewModel
internal class ProductGamePillOverlayViewModel @Inject constructor(
    reminder: RealProductGameReminder,
    private val router: VideoGameRouter,
) : BaseViewModel() {
    private class Countdown(val game: ScheduledProductGame, val secondsLeft: Long)

    private val countdown: StateFlow<Countdown?> =
        combine(reminder.scheduled, currentTimestampFlow()) { games, now ->
            games.mapNotNull { game -> game.countdownAt(now) }.minByOrNull { it.game.startsAtMillis }
        }.stateInBackground(SharingStarted.WhileSubscribed(), null)

    val secondsLeft: StateFlow<Long?> = countdown
        .map { it?.secondsLeft }
        .stateInBackground(SharingStarted.WhileSubscribed(), null)

    fun onPillClicked() = launchUnit {
        countdown.value?.let { router.openGameProduct(it.game.product()) }
    }

    private fun ScheduledProductGame.countdownAt(nowMillis: Long): Countdown? {
        val untilStart = (startsAtMillis - nowMillis).milliseconds
        val inWindow = untilStart.isPositive() && untilStart <= PILL_SHOWN_BEFORE_START
        return if (inWindow) Countdown(this, untilStart.inWholeSeconds) else null
    }
}
