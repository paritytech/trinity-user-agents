package io.paritytech.polkadotapp.design.components.docklet

import android.os.Build
import androidx.activity.BackEventCompat

enum class DockletBackEdge { Left, Right, None }

enum class DockletBackAction { Claim, Restore, PlainBack }

fun dockletBackAction(
    edge: DockletBackEdge,
    touchY: Float,
    band: ClosedFloatingPointRange<Float>,
    restoresOnBack: Boolean,
): DockletBackAction = when {
    edge == DockletBackEdge.None -> dockletPressBackAction(restoresOnBack)
    edge == DockletBackEdge.Right && touchY in band -> DockletBackAction.Claim
    else -> DockletBackAction.PlainBack
}

fun dockletPressBackAction(restoresOnBack: Boolean): DockletBackAction =
    if (restoresOnBack) DockletBackAction.Restore else DockletBackAction.PlainBack

// Before Android 15 a Back button press reports the left edge, so only a swipe there is trusted to be one.
fun dockletBackEdge(swipeEdge: Int, sdkInt: Int): DockletBackEdge = when (swipeEdge) {
    BackEventCompat.EDGE_RIGHT -> DockletBackEdge.Right
    BackEventCompat.EDGE_LEFT -> if (sdkInt >= Build.VERSION_CODES.VANILLA_ICE_CREAM) DockletBackEdge.Left else DockletBackEdge.None
    else -> DockletBackEdge.None
}
