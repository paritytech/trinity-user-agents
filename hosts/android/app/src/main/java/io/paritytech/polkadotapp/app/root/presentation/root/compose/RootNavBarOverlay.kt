package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarBaseInset
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
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
private val SWIPE_AREA_EXPANSION = 32.dp

// Side margins of the open bar. Applied so the measured bar width ends at the pill's right edge — the
// collapsed nub then stays exactly [NUB_WIDTH] wide and flush to the screen edge (the left margin only
// exists while the bar is pulled out).
private val BAR_HORIZONTAL_MARGIN = 16.dp

// Horizontal fling faster than this (dp per second) settles the bar in the fling direction regardless of
// how far it was dragged. Matches Material's swipeable velocity threshold.
private val FLING_VELOCITY_THRESHOLD = 125.dp

private const val BACKDROP_ALPHA = 0.7f

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

    openScanPanelRequests.collectAsEffect { _, _ ->
        onScannerTooltipDismiss()
        pull.openScan()
    }

    val scrimVisible by remember(hidden, forceShown) {
        derivedStateOf { !hidden && (pull.panelExpanded || (pull.isOpen && !forceShown)) }
    }

    val tooltipVisible by remember(hidden, scannerTooltipVisible) {
        derivedStateOf { !hidden && scannerTooltipVisible && pull.isFullyOpen }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        AnimatedVisibility(
            visible = !hidden && pull.scanExpanded,
            enter = fadeIn(),
            exit = fadeOut(),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(PolkadotTheme.colors.bg.surface.overlay.copy(alpha = BACKDROP_ALPHA)),
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
            modifier = Modifier.align(Alignment.BottomCenter),
            enter = slideInHorizontally(initialOffsetX = edgeOffset) + fadeIn(),
            exit = slideOutHorizontally(targetOffsetX = edgeOffset) + fadeOut(),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .statusBarsPadding()
                    .graphicsLayer { translationX = -pull.offset },
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .systemGestureExclusion()
                        .pointerInput(pull.maxOffset, forceShown) {
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
                        .grabBandAbove(SWIPE_AREA_EXPANSION),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    RootNavBar(
                        // Left margin, then measure (→ pill's right edge = width − right margin so the nub
                        // math is unchanged), then right margin.
                        modifier = Modifier
                            .padding(start = BAR_HORIZONTAL_MARGIN)
                            .onSizeChanged { pull.setBarWidth(it.width.toFloat()) }
                            .padding(end = BAR_HORIZONTAL_MARGIN),
                        currentTab = currentTab,
                        tabWarnings = tabWarnings,
                        apps = openApps,
                        appsExpanded = pull.appsExpanded,
                        chainsHealth = chainsHealth,
                        networkStatusExpanded = pull.networkStatusExpanded,
                        scannerTooltipVisible = tooltipVisible,
                        scanExpanded = pull.scanExpanded,
                        // Selecting the tab you are already on adds no back-stack entry, so the
                        // panel would otherwise stay up with its tab deselected.
                        onTabSelected = { tab -> pull.collapsePanels(); onTabSelected(tab) },
                        onCountClicked = { pull.toggleApps() },
                        onNetworkStatusClicked = { pull.toggleNetworkStatus() },
                        onAppClick = onAppClick,
                        onAppClose = onAppClose,
                        onScanClicked = { onScannerTooltipDismiss(); pull.toggleScan() },
                        onScanHandled = { navigate -> pull.collapsePanels(); navigate?.invoke() },
                        onScanDismiss = { pull.collapsePanels() },
                        onScannerTooltipDismiss = onScannerTooltipDismiss,
                        onRestingHeightChange = { if (!hidden) onBarHeight(it) },
                    )
                }
            }
        }
    }
}

private fun Modifier.grabBandAbove(band: Dp): Modifier = layout { measurable, constraints ->
    val placeable = measurable.measure(constraints.copy(minHeight = 0))
    val height = (placeable.height + band.roundToPx()).coerceIn(constraints.minHeight, constraints.maxHeight)

    layout(placeable.width, height) {
        placeable.place(0, height - placeable.height)
    }
}
