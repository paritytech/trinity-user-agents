package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

internal sealed interface VideoGamePillState {
    data object Hidden : VideoGamePillState

    sealed interface Shown : VideoGamePillState {
        data class WaitingCountdown(val secondsLeft: Long) : Shown
        data class InProgress(val currentRound: Int, val totalRounds: Int) : Shown
        data object Review : Shown
    }
}
