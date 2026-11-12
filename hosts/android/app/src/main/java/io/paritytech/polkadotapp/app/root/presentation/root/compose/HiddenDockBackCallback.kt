package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.activity.BackEventCompat
import androidx.activity.OnBackPressedCallback
import androidx.activity.OnBackPressedDispatcher
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.Density
import io.paritytech.polkadotapp.design.components.docklet.DockletHideState
import io.paritytech.polkadotapp.design.components.docklet.dockletPoint
import kotlin.time.Duration

class HiddenDockBackCallback(
    private val hideState: DockletHideState,
    private val density: Density,
    private val dispatcher: OnBackPressedDispatcher,
    private val restoresOnBack: Boolean,
    private val uptime: () -> Duration,
) : OnBackPressedCallback(true) {
    private var pullsDock = false

    override fun handleOnBackStarted(backEvent: BackEventCompat) {
        val point = backEvent.dockletPoint()
        pullsDock = backEvent.swipeEdge == BackEventCompat.EDGE_RIGHT && point.y in hideState.drawbridge.backBand()
        if (pullsDock) hideState.startRestore(point, uptime())
    }

    override fun handleOnBackProgressed(backEvent: BackEventCompat) {
        if (pullsDock) hideState.moveRestore(backEvent.dockletPoint(), uptime())
    }

    override fun handleOnBackCancelled() = settlePull()

    override fun handleOnBackPressed() {
        if (pullsDock) settlePull() else passOn()
    }

    private fun settlePull() {
        if (pullsDock) hideState.cancelDrag()
        pullsDock = false
    }

    private fun passOn() {
        isEnabled = false
        try {
            if (restoresOnBack && !dispatcher.hasEnabledCallbacks()) hideState.restore() else dispatcher.onBackPressed()
        } finally {
            isEnabled = true
        }
    }

    private fun BackEventCompat.dockletPoint() = density.dockletPoint(Offset(touchX, touchY))
}
