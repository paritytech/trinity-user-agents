package io.paritytech.polkadotapp.app.root.presentation.root.compose

import android.app.Activity
import android.os.Build
import android.view.View
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.annotation.RequiresApi
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.tabbar.DockletSlotAction
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarBaseInset
import io.paritytech.polkadotapp.common.presentation.tabbar.dockletSlotTap
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.design.components.docklet.DockletMaterials
import io.paritytech.polkadotapp.design.components.docklet.DockletStep
import io.paritytech.polkadotapp.design.components.docklet.dockletBackdrop
import io.paritytech.polkadotapp.design.components.docklet.rememberDockletBackdrop
import io.paritytech.polkadotapp.design.components.docklet.rememberDockletCardState
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.coroutines.flow.Flow
import kotlin.math.abs

private val NUB_WIDTH = TabBarBaseInset

// On press the bar pops out this far so its edge sits under the finger, ready to be dragged (a plain
// tap without a drag still opens it fully).
private val PRESS_PEEK = 48.dp

// Extra grab band above the pill so the bar is easy to catch even off its visible edge.
internal val SWIPE_AREA_EXPANSION = 32.dp

// Horizontal fling faster than this (dp per second) settles the bar in the fling direction regardless of
// how far it was dragged. Matches Material's swipeable velocity threshold.
private val FLING_VELOCITY_THRESHOLD = 125.dp

private const val BEHIND_CARD_DIM = 1f - DockletMaterials.BehindCardBrightness

/**
 * Hosts the global navigation bar as a right-edge pull-out. The bar sits off-screen with only a [NUB_WIDTH]
 * nub showing; dragging it left reveals it, and it springs into place on release. Tapping outside collapses
 * it. Its current intrusion (dp) is reported via [onOffset] so screens can shift aside. When [hidden] it is
 * not shown at all. On each screen entry ([screenKey] changes) the bar snaps back to its default posture.
 */
@Composable
fun RootNavBarOverlay(
    hidden: Boolean,
    forceShown: Boolean,
    screenKey: String?,
    currentTab: BottomTab,
    tabWarnings: ImmutableMap<BottomTab, Boolean>,
    openApps: ImmutableList<TabInfo>,
    chainsHealth: ChainHealthIndicatorsModel,
    scannerTooltipVisible: Boolean,
    openScanPanelRequests: Flow<Unit>,
    backdropSource: View?,
    onTabSelected: (BottomTab) -> Unit,
    onScannerTooltipDismiss: () -> Unit,
    onAppClick: (Long) -> Unit,
    onAppClose: (Long) -> Unit,
    onOffset: (Dp) -> Unit,
    onBarHeight: (Dp) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val nubPx = with(density) { NUB_WIDTH.toPx() }
    val flingVelocityPx = with(density) { FLING_VELOCITY_THRESHOLD.toPx() }
    val pull = remember(scope, nubPx, flingVelocityPx) { TabBarPullState(scope, nubPx, flingVelocityPx) }
    val cardState = rememberDockletCardState()
    val backdrop = rememberDockletBackdrop(backdropSource)
    val focusManager = LocalFocusManager.current
    var keyboardRequested by remember { mutableStateOf(false) }

    // Snap to the screen's default posture on first measure, on force toggle, and on every screen entry, so
    // a screen never inherits the previous one's pulled-out position.
    LaunchedEffect(pull.maxOffset, forceShown, screenKey) {
        pull.snapToDefault(forceShown)
    }
    // Report the bar's visible width off snapshots, so the per-frame drag doesn't recompose the overlay.
    LaunchedEffect(Unit) {
        snapshotFlow { pull.visibleWidthPx }.collect { onOffset(with(density) { it.toDp() }) }
    }
    // Both intrusions have to be surrendered on hide, not just the horizontal one: the height is only ever
    // measured while the bar is composed, so without this screens keep reserving the band it used to
    // occupy — and ChatExtensionOverlayHost reads a non-zero height as "a bar is there" and drops its own
    // navigation-bar padding.
    LaunchedEffect(hidden) {
        if (hidden) {
            onOffset(0.dp)
            onBarHeight(0.dp)
        }
    }

    val openCard: (DockletStep) -> Unit = { step ->
        onScannerTooltipDismiss()
        cardState.open(step)
        pull.openScan()
    }

    openScanPanelRequests.collectAsEffect { _, _ -> openCard(DockletStep.Step1) }

    LaunchedEffect(pull.scanExpanded) {
        if (pull.scanExpanded) {
            cardState.open(DockletStep.Step1)
        } else {
            keyboardRequested = false
            focusManager.clearFocus()
            cardState.close()
        }
    }

    val cardOpen = !hidden && pull.scanExpanded
    // Composed only while open, so it registers after the screens' own BackHandlers and runs before them.
    if (cardOpen) {
        BackHandler { pull.collapsePanels() }
        OverlayBackCallback { pull.collapsePanels() }
    }

    val scrimVisible by remember(hidden, forceShown) {
        derivedStateOf { !hidden && (pull.panelExpanded || (pull.isOpen && !forceShown)) }
    }

    val tooltipVisible by remember(hidden, scannerTooltipVisible) {
        derivedStateOf { !hidden && scannerTooltipVisible && pull.isFullyOpen }
    }

    val shade = PolkadotTheme.colors.bg.surface.overlay
    Box(modifier = Modifier.fillMaxSize()) {
        if (!hidden) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .dockletBackdrop(
                        backdrop = backdrop,
                        shape = RectangleShape,
                        sigma = { DockletMaterials.BehindCardBlurSigma * cardState.clampedProgress },
                    )
                    .drawBehind {
                        drawRect(shade.copy(alpha = BEHIND_CARD_DIM * cardState.clampedProgress))
                    },
            )
        }

        if (scrimVisible) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null,
                    ) {
                        if (pull.panelExpanded) pull.collapsePanels() else pull.collapse()
                    },
            )
        }

        val edgeOffset: (Int) -> Int = { -it }
        AnimatedVisibility(
            visible = !hidden,
            enter = slideInHorizontally(initialOffsetX = edgeOffset) + fadeIn(),
            exit = slideOutHorizontally(targetOffsetX = edgeOffset) + fadeOut(),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .graphicsLayer { translationX = -pull.offset },
            ) {
                RootNavBar(
                    cardState = cardState,
                    backdrop = backdrop,
                    currentTab = currentTab,
                    tabWarnings = tabWarnings,
                    apps = openApps,
                    appsExpanded = pull.appsExpanded,
                    chainsHealth = chainsHealth,
                    networkStatusExpanded = pull.networkStatusExpanded,
                    scannerTooltipVisible = tooltipVisible,
                    keyboardRequested = keyboardRequested,
                    pullState = pull,
                    forceShown = forceShown,
                    onSlotClick = { target ->
                        when (val action = dockletSlotTap(target = target, currentTab = currentTab, onMain = forceShown, cardOpen = pull.scanExpanded)) {
                            DockletSlotAction.CloseCard -> pull.collapsePanels()
                            DockletSlotAction.ToggleApps -> pull.toggleApps()
                            DockletSlotAction.ToggleNetworkStatus -> pull.toggleNetworkStatus()
                            // Selecting the tab you are already on adds no back-stack entry, so the
                            // panel would otherwise stay up with its tab deselected.
                            is DockletSlotAction.SelectTab -> {
                                pull.collapsePanels()
                                onTabSelected(action.tab)
                            }
                        }
                    },
                    onOpenCard = openCard,
                    onCloseCard = {
                        pull.collapsePanels()
                        cardState.close()
                    },
                    onCardStepChange = { step ->
                        cardState.changeStep(step)
                        keyboardRequested = step == DockletStep.Step2
                        if (step == DockletStep.Step1) focusManager.clearFocus()
                    },
                    onKeyboardRequest = { keyboardRequested = true },
                    onAppClick = onAppClick,
                    onAppClose = onAppClose,
                    onScanHandled = { navigate -> pull.collapsePanels(); navigate?.invoke() },
                    onScannerTooltipDismiss = onScannerTooltipDismiss,
                    onRestingHeightChange = { if (!hidden) onBarHeight(it) },
                    onBarWidthChange = pull::setBarWidth,
                )
            }
        }
    }
}

// Outranks the keyboard's default-priority back callback, so one Back closes the card and the keyboard.
@Composable
private fun OverlayBackCallback(onBack: () -> Unit) {
    val activity = LocalActivity.current
    val currentOnBack by rememberUpdatedState(onBack)
    DisposableEffect(activity) {
        val unregister = if (activity != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.BAKLAVA) {
            registerOverlayBack(activity) { currentOnBack() }
        } else {
            null
        }
        onDispose { unregister?.invoke() }
    }
}

@RequiresApi(Build.VERSION_CODES.BAKLAVA)
private fun registerOverlayBack(activity: Activity, onBack: () -> Unit): () -> Unit {
    val dispatcher = activity.onBackInvokedDispatcher
    val callback = OnBackInvokedCallback { onBack() }
    dispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_OVERLAY, callback)
    return { dispatcher.unregisterOnBackInvokedCallback(callback) }
}

internal fun Modifier.pullOutGestures(pull: TabBarPullState, forceShown: Boolean, density: Density): Modifier =
    pointerInput(pull.maxOffset, forceShown) {
        if (forceShown || pull.maxOffset <= 0f) return@pointerInput
        val peekPx = with(density) { PRESS_PEEK.toPx() }
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val collapsed = pull.peek(peekPx)
            val pass = if (collapsed) PointerEventPass.Initial else PointerEventPass.Main
            if (collapsed) down.consume()
            val startX = down.position.x
            var dragging = false
            val velocityTracker = VelocityTracker()
            velocityTracker.addPointerInputChange(down)
            while (true) {
                val change = awaitPointerEvent(pass).changes.firstOrNull { it.id == down.id } ?: break
                if (!change.pressed) {
                    if (collapsed) change.consume()
                    break
                }
                velocityTracker.addPointerInputChange(change)
                if (!dragging && abs(change.position.x - startX) > viewConfiguration.touchSlop) {
                    dragging = true
                }
                if (dragging) {
                    // Dragging right (from the left nub) opens the bar.
                    pull.dragBy(-change.positionChange().x)
                    change.consume()
                } else if (collapsed) {
                    change.consume()
                }
            }
            when {
                dragging -> pull.settleAfterDrag(-velocityTracker.calculateVelocity().x)
                collapsed -> pull.openFromNub()
                else -> pull.undoPeek()
            }
        }
    }

internal fun Modifier.grabBandAbove(band: Dp): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints.copy(minHeight = 0))
    val height = (placeable.height + band.roundToPx()).coerceIn(constraints.minHeight, constraints.maxHeight)

    layout(placeable.width, height) {
        placeable.place(0, height - placeable.height)
    }
}
