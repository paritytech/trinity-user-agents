package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.feature_videogame_impl.utils.VideoGameLaunchCoordinator
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import javax.inject.Inject

@HiltViewModel
internal class WeeklyGamePillOverlayViewModel @Inject constructor(
    private val launchCoordinator: VideoGameLaunchCoordinator,
    pillStateProducer: WeeklyGamePillStateProducer,
    pillVisibility: WeeklyGamePillVisibilityHolder,
) : BaseViewModel(), GamePillViewModel {
    override val pillState: StateFlow<VideoGamePillState> = combine(
        pillStateProducer.pillState(),
        pillVisibility.footerVisible,
        pillVisibility.inlinePillVisible,
    ) { state, footerVisible, inlinePillVisible ->
        if (footerVisible || inlinePillVisible) VideoGamePillState.Hidden else state
    }.stateInBackground(SharingStarted.Eagerly, VideoGamePillState.Hidden)

    override fun onPillClicked() = launchUnit {
        launchCoordinator.openOrLaunchGame()
    }
}
