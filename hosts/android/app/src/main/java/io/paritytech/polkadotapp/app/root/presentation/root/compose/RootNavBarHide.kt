package io.paritytech.polkadotapp.app.root.presentation.root.compose

import android.os.SystemClock
import android.view.accessibility.AccessibilityManager
import androidx.activity.BackEventCompat
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import io.paritytech.polkadotapp.app.root.domain.main.DockletTip
import io.paritytech.polkadotapp.app.root.presentation.main.compose.components.DockletTipPopup
import io.paritytech.polkadotapp.app.root.presentation.root.DockletInput
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarTuckedPin
import io.paritytech.polkadotapp.design.components.docklet.DockletAutoHide
import io.paritytech.polkadotapp.design.components.docklet.DockletAutoHideBlockers
import io.paritytech.polkadotapp.design.components.docklet.DockletEdgeGlow
import io.paritytech.polkadotapp.design.components.docklet.DockletFrame
import io.paritytech.polkadotapp.design.components.docklet.DockletGlowMasks
import io.paritytech.polkadotapp.design.components.docklet.DockletHideState
import io.paritytech.polkadotapp.design.components.docklet.dockletPoint
import io.paritytech.polkadotapp.design.components.docklet.dockletTouch
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import kotlinx.coroutines.flow.Flow
import kotlin.coroutines.cancellation.CancellationException
import kotlin.time.Duration.Companion.milliseconds
import io.paritytech.polkadotapp.common.R as RCommon

private val TipAnchorWidth = 1.dp

@Composable
fun RootNavBarHide(
    hideState: DockletHideState,
    searchOpen: Boolean,
    tuckedPin: TabBarTuckedPin?,
    glowMasks: DockletGlowMasks,
    panelExpanded: Boolean,
    scannerTooltipVisible: Boolean,
    dockletTip: DockletTip?,
    dockletAreas: () -> List<DockletFrame>,
    inputs: Flow<DockletInput>,
    onTipDismiss: () -> Unit,
) {
    val density = LocalDensity.current
    val talkBack = rememberTouchExplorationEnabled()
    val lifecycleState by LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    val resumed = lifecycleState.isAtLeast(Lifecycle.State.RESUMED)
    val ime = WindowInsets.ime
    val imeUp by remember(ime, density) { derivedStateOf { ime.getBottom(density) > 0 } }
    val shownAtRest by remember(hideState) { derivedStateOf { hideState.isShownAtRest } }
    val scope = rememberCoroutineScope()
    val autoHide = remember(hideState, scope) { DockletAutoHide(scope) { hideState.quietHide() } }
    val currentAreas by rememberUpdatedState(dockletAreas)

    val shownTip = dockletTip.takeIf { hideState.countsAsHidden }
    val blockers = DockletAutoHideBlockers(
        shownAtRest = shownAtRest,
        searchOpen = searchOpen,
        keyboardUp = imeUp,
        tipShown = shownTip != null || scannerTooltipVisible,
        panelExpanded = panelExpanded,
        touchExploration = talkBack,
        resumed = resumed,
    )
    SideEffect { autoHide.blocked = blockers.blocked }

    LaunchedEffect(hideState.countsAsHidden, dockletTip) {
        if (!hideState.countsAsHidden && dockletTip != null) onTipDismiss()
    }

    inputs.collectAsEffect { _, input ->
        when (input) {
            is DockletInput.Touch -> autoHide.onTouch(dockletTouch(density.dockletPoint(Offset(input.x, input.y)), currentAreas()))
            DockletInput.Key -> autoHide.onKey()
            DockletInput.Wheel -> autoHide.onWheel()
        }
    }

    var unclaimedBack by remember { mutableStateOf(false) }
    if (hideState.countsAsHidden && !searchOpen && !unclaimedBack) {
        // A screen's own Back callback registers when it starts; re-registering on each tucked screen keeps this one on top.
        key(tuckedPin) {
            HiddenDockBack(hideState = hideState, onUnclaimedBack = { unclaimedBack = true })
        }
    }
    val dispatcher = LocalOnBackPressedDispatcherOwner.current?.onBackPressedDispatcher
    val restoresOnBack = tuckedPin == null
    LaunchedEffect(unclaimedBack) {
        if (unclaimedBack) {
            // The handler above must leave composition first, so only the callbacks behind it are left.
            withFrameNanos { }
            withFrameNanos { }
            if (restoresOnBack && dispatcher?.hasEnabledCallbacks() != true) hideState.restore() else dispatcher?.onBackPressed()
            unclaimedBack = false
        }
    }

    val drawbridge = hideState.drawbridge
    if (drawbridge != null) {
        DockletEdgeGlow(state = hideState, grabZone = drawbridge.grabZone, masks = glowMasks, label = stringResource(RCommon.string.docklet_show_dock))
        val glow = drawbridge.glowFrame
        Box(modifier = Modifier.offset(glow.left, glow.top).size(TipAnchorWidth, glow.height)) {
            DockletTipPopup(tip = shownTip, onRestore = hideState::restore, onDismiss = onTipDismiss)
        }
    }
}

@Composable
private fun HiddenDockBack(hideState: DockletHideState, onUnclaimedBack: () -> Unit) {
    val density = LocalDensity.current
    val currentOnUnclaimedBack by rememberUpdatedState(onUnclaimedBack)

    PredictiveBackHandler { events ->
        var route: HiddenDockBackRoute? = null
        try {
            events.collect { event ->
                val point = density.dockletPoint(Offset(event.touchX, event.touchY))
                if (route == null) {
                    val band = hideState.drawbridge?.backBand() ?: return@collect
                    val pulls = event.swipeEdge == BackEventCompat.EDGE_RIGHT && point.y in band
                    route = if (pulls) HiddenDockBackRoute.PullsDock else HiddenDockBackRoute.PassesOn
                    if (pulls) hideState.startRestore(point, SystemClock.uptimeMillis().milliseconds)
                }
                if (route == HiddenDockBackRoute.PullsDock) hideState.moveRestore(point, SystemClock.uptimeMillis().milliseconds)
            }
            if (route == HiddenDockBackRoute.PullsDock) hideState.cancelDrag() else currentOnUnclaimedBack()
        } catch (cancelled: CancellationException) {
            if (route == HiddenDockBackRoute.PullsDock) hideState.cancelDrag()
            throw cancelled
        }
    }
}

private enum class HiddenDockBackRoute { PullsDock, PassesOn }

@Composable
private fun rememberTouchExplorationEnabled(): Boolean {
    val context = LocalContext.current
    var enabled by remember { mutableStateOf(false) }
    DisposableEffect(context) {
        val manager = context.getSystemService(AccessibilityManager::class.java)
        enabled = manager.isTouchExplorationEnabled
        val listener = AccessibilityManager.TouchExplorationStateChangeListener { enabled = it }
        manager.addTouchExplorationStateChangeListener(listener)
        onDispose { manager.removeTouchExplorationStateChangeListener(listener) }
    }
    return enabled
}
