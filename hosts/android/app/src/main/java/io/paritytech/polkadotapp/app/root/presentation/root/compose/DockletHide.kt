package io.paritytech.polkadotapp.app.root.presentation.root.compose

import android.os.SystemClock
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import io.paritytech.polkadotapp.app.root.domain.main.DockletTip
import io.paritytech.polkadotapp.app.root.presentation.main.compose.components.DockletTipPopup
import io.paritytech.polkadotapp.app.root.presentation.root.DockletInput
import io.paritytech.polkadotapp.design.components.docklet.DockletAutoHide
import io.paritytech.polkadotapp.design.components.docklet.DockletAutoHideBlockers
import io.paritytech.polkadotapp.design.components.docklet.DockletEdgeGlow
import io.paritytech.polkadotapp.design.components.docklet.DockletFrame
import io.paritytech.polkadotapp.design.components.docklet.DockletGlowMasks
import io.paritytech.polkadotapp.design.components.docklet.DockletHideState
import io.paritytech.polkadotapp.design.components.docklet.dockletPoint
import io.paritytech.polkadotapp.design.components.docklet.dockletTouch
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.design.utils.rememberTouchExplorationEnabled
import kotlinx.coroutines.flow.Flow
import kotlin.time.Duration.Companion.milliseconds
import io.paritytech.polkadotapp.common.R as RCommon

private val TipAnchorWidth = 1.dp

@Composable
fun DockletAutoHideEffect(
    hideState: DockletHideState,
    searchOpen: Boolean,
    tipShown: Boolean,
    panelExpanded: Boolean,
    dockletAreas: () -> List<DockletFrame>,
    inputs: Flow<DockletInput>,
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

    val blockers = DockletAutoHideBlockers(
        shownAtRest = shownAtRest,
        searchOpen = searchOpen,
        keyboardUp = imeUp,
        tipShown = tipShown,
        panelExpanded = panelExpanded,
        touchExploration = talkBack,
        resumed = resumed,
    )
    SideEffect { autoHide.blocked = blockers.blocked }

    inputs.collectAsEffect { _, input ->
        when (input) {
            is DockletInput.Touch -> autoHide.onTouch(dockletTouch(density.dockletPoint(Offset(input.x, input.y)), currentAreas()))
            DockletInput.Key -> autoHide.onKey()
            DockletInput.Wheel -> autoHide.onWheel()
        }
    }
}

@Composable
fun HiddenDockGlow(hideState: DockletHideState, glowMasks: DockletGlowMasks, dockletTip: DockletTip?, onTipDismiss: () -> Unit) {
    LaunchedEffect(hideState.countsAsHidden, dockletTip) {
        if (!hideState.countsAsHidden && dockletTip != null) onTipDismiss()
    }

    val drawbridge = hideState.drawbridge
    DockletEdgeGlow(state = hideState, grabZone = drawbridge.grabZone, masks = glowMasks, label = stringResource(RCommon.string.docklet_show_dock))
    val glow = drawbridge.glowFrame
    Box(modifier = Modifier.offset(glow.left, glow.top).size(TipAnchorWidth, glow.height)) {
        DockletTipPopup(tip = dockletTip.takeIf { hideState.countsAsHidden }, onRestore = hideState::restore, onDismiss = onTipDismiss)
    }
}

@Composable
fun HiddenDockBack(hideState: DockletHideState, restoresOnBack: Boolean) {
    val dispatcher = LocalOnBackPressedDispatcherOwner.current?.onBackPressedDispatcher
    val lifecycleOwner = LocalLifecycleOwner.current
    val density = LocalDensity.current
    if (dispatcher != null) {
        DisposableEffect(dispatcher, lifecycleOwner, hideState, density, restoresOnBack) {
            val callback = HiddenDockBackCallback(
                hideState = hideState,
                density = density,
                dispatcher = dispatcher,
                restoresOnBack = restoresOnBack,
                uptime = { SystemClock.uptimeMillis().milliseconds },
            )
            dispatcher.addCallback(lifecycleOwner, callback)
            onDispose { callback.remove() }
        }
    }
}
