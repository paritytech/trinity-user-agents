package io.paritytech.polkadotapp.design.components.bottomsheet

import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith

object NovaBottomSheetDefaults {
    const val STEP_TRANSITION_EXIT_DURATION = 90
    const val STEP_TRANSITION_ENTER_DURATION = 220

    val PAGE_TRANSITION_SPEC = fadeIn(
        animationSpec = tween(
            durationMillis = STEP_TRANSITION_ENTER_DURATION,
            delayMillis = STEP_TRANSITION_EXIT_DURATION
        )
    ) togetherWith fadeOut(animationSpec = tween(STEP_TRANSITION_EXIT_DURATION))
}
