package io.paritytech.polkadotapp.design.components.docklet

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView

enum class DockletMoment { Open, SwipeStart, Close, StepChange, SlotTap, HideCross, Hidden, Restored }

fun dockletHapticConstant(moment: DockletMoment, sdkInt: Int): Int = when (moment) {
    DockletMoment.Open,
    DockletMoment.Restored -> if (sdkInt >= Build.VERSION_CODES.R) {
        HapticFeedbackConstants.CONFIRM
    } else {
        HapticFeedbackConstants.CONTEXT_CLICK
    }

    DockletMoment.SwipeStart,
    DockletMoment.Close,
    DockletMoment.SlotTap,
    DockletMoment.HideCross -> HapticFeedbackConstants.CLOCK_TICK

    DockletMoment.Hidden -> HapticFeedbackConstants.LONG_PRESS

    DockletMoment.StepChange -> HapticFeedbackConstants.CONTEXT_CLICK
}

fun interface DockletHaptics {
    fun perform(moment: DockletMoment)
}

@Composable
fun rememberDockletHaptics(): DockletHaptics {
    val view = LocalView.current
    return remember(view) {
        DockletHaptics { moment -> view.performHapticFeedback(dockletHapticConstant(moment, Build.VERSION.SDK_INT)) }
    }
}
