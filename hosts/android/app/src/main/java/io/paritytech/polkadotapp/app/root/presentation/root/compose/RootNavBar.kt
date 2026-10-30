package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.tappableElement
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.lerp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import io.paritytech.polkadotapp.app.root.presentation.main.compose.components.ScannerTooltip
import io.paritytech.polkadotapp.app.root.presentation.main.compose.components.rememberScannerIconColor
import io.paritytech.polkadotapp.app.root.presentation.main.compose.icon
import io.paritytech.polkadotapp.app.root.presentation.main.compose.title
import io.paritytech.polkadotapp.app.root.presentation.root.compose.components.ScanPanel
import io.paritytech.polkadotapp.common.presentation.tabbar.DockletSlotTarget
import io.paritytech.polkadotapp.common.presentation.tabbar.dockletHighlightedSlot
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.design.components.docklet.Docklet
import io.paritytech.polkadotapp.design.components.docklet.DockletBackdrop
import io.paritytech.polkadotapp.design.components.docklet.DockletCard
import io.paritytech.polkadotapp.design.components.docklet.DockletCardFrames
import io.paritytech.polkadotapp.design.components.docklet.DockletCardState
import io.paritytech.polkadotapp.design.components.docklet.DockletFrame
import io.paritytech.polkadotapp.design.components.docklet.DockletGeometry
import io.paritytech.polkadotapp.design.components.docklet.DockletSlot
import io.paritytech.polkadotapp.design.components.docklet.DockletStep
import io.paritytech.polkadotapp.design.components.docklet.liftDockWhileKeyboardLeaves
import io.paritytech.polkadotapp.design.components.docklet.rememberDockletKeyboardHeight
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.NetworkStatus
import io.paritytech.polkadotapp.design.components.icon.vectors.Trash
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.AddContactStep
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthPanel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

private const val APPS_PER_ROW = 5
private val AppIconSize = 24.dp

// Height of the empty-state block — roughly one row of app cells (icon + label), so the container looks
// the same size whether it shows one row of products or the "no products" hint.
private val EmptyAppsRowHeight = 44.dp

private val PanelCornerRadius = 32.dp

private const val CAMERA_START_AT_OPENING_FRACTION = 0.3f

private val AppMenuIconSize = 20.dp
private val AppMenuShadowElevation = 8.dp

private val PillBodyHeight = DockletGeometry.PillDiameter - DockletGeometry.LobeDepth

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun RootNavBar(
    cardState: DockletCardState,
    backdrop: DockletBackdrop?,
    currentTab: BottomTab,
    tabWarnings: ImmutableMap<BottomTab, Boolean>,
    apps: ImmutableList<TabInfo>,
    appsExpanded: Boolean,
    chainsHealth: ChainHealthIndicatorsModel,
    networkStatusExpanded: Boolean,
    scannerTooltipVisible: Boolean,
    keyboardRequested: Boolean,
    pullState: TabBarPullState,
    forceShown: Boolean,
    onSlotClick: (DockletSlotTarget) -> Unit,
    onOpenCard: (DockletStep) -> Unit,
    onCloseCard: () -> Unit,
    onCardStepChange: (DockletStep) -> Unit,
    onKeyboardRequest: () -> Unit,
    onAppClick: (Long) -> Unit,
    onAppClose: (Long) -> Unit,
    onScanHandled: (navigate: (() -> Unit)?) -> Unit,
    onScannerTooltipDismiss: () -> Unit,
    onRestingHeightChange: (Dp) -> Unit,
    onBarWidthChange: (Float) -> Unit,
) {
    val density = LocalDensity.current
    val navigationInset = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val threeButtonNavigation = WindowInsets.tappableElement.asPaddingValues().calculateBottomPadding() > 0.dp
    val statusInset = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val keyboardHeight = rememberDockletKeyboardHeight()
    val networkStatusItem = FeatureOption.TAB_BAR_CONNECTIVITY_INDICATOR.isEnabled
    val currentOnRestingHeightChange by rememberUpdatedState(onRestingHeightChange)
    val currentOnBarWidthChange by rememberUpdatedState(onBarWidthChange)

    BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
        val dock = DockletGeometry.dockFrame(maxWidth, maxHeight, navigationInset, threeButtonNavigation)
        val screenHeight = maxHeight
        val keyboardTop = { screenHeight - keyboardHeight() }
        var dockOnKeyboard by remember { mutableStateOf(false) }
        LaunchedEffect(cardState, keyboardHeight) {
            liftDockWhileKeyboardLeaves(
                cardOpen = snapshotFlow { cardState.isOpen },
                keyboardUp = snapshotFlow { keyboardHeight() > 0.dp },
                onLift = { dockOnKeyboard = it },
            )
        }
        val liftedDock = { if (dockOnKeyboard) DockletGeometry.keyboardDock(dock, keyboardTop()) else dock }
        val frames: () -> DockletCardFrames = remember(dock, statusInset) {
            {
                val lifted = liftedDock()
                val pill = DockletGeometry.pillFrame(lifted)
                DockletCardFrames(
                    rest = DockletFrame(pill.left, pill.top, pill.width, PillBodyHeight),
                    step1 = DockletGeometry.step1CardFrame(lifted),
                    step2 = DockletGeometry.step2CardFrame(dock, keyboardTop(), statusInset),
                )
            }
        }
        val restingHeight = maxHeight - dock.top + DockletGeometry.PillProud
        LaunchedEffect(restingHeight) { currentOnRestingHeightChange(restingHeight) }
        LaunchedEffect(dock.right, density) { currentOnBarWidthChange(with(density) { dock.right.toPx() }) }

        val highlighted = dockletHighlightedSlot(
            currentTab = currentTab,
            networkStatusExpanded = networkStatusItem && networkStatusExpanded,
        )
        val targets = remember(networkStatusItem) {
            (BottomTab.availableEntries.map(DockletSlotTarget::Tab) +
                listOfNotNull(DockletSlotTarget.NetworkStatus.takeIf { networkStatusItem })).toImmutableList()
        }
        val titles = targets.map { it.title() }
        val currentOnSlotClick by rememberUpdatedState(onSlotClick)
        val slots = remember(targets, titles, highlighted, apps.size, tabWarnings) {
            targets.mapIndexed { index, target ->
                DockletSlot(
                    icon = target.icon(),
                    contentDescription = titles[index],
                    selected = target == highlighted,
                    badgeCount = if (target == DockletSlotTarget.Tab(BottomTab.EXPLORE)) apps.size else 0,
                    warning = target is DockletSlotTarget.Tab && tabWarnings[target.tab] == true,
                    onClick = { currentOnSlotClick(target) },
                )
            }.toImmutableList()
        }

        Column(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(start = dock.left, bottom = maxHeight - dock.bottom)
                .width(dock.width)
                .systemGestureExclusion()
                .pullOutGestures(pullState, forceShown, density)
                .grabBandAbove(SWIPE_AREA_EXPANSION),
        ) {
            ExpandablePanel(visible = appsExpanded) {
                OpenAppsRow(apps = apps, onAppClick = onAppClick, onAppClose = onAppClose)
            }
            if (networkStatusItem) {
                ExpandablePanel(visible = networkStatusExpanded) {
                    ChainHealthPanel(model = chainsHealth)
                }
            }
            VerticalSpacer { DockletGeometry.PillProud }
            Docklet(
                modifier = Modifier
                    .fillMaxWidth()
                    .offset {
                        val lifted = liftedDock()
                        val open = lerp(
                            DockletGeometry.step1DockTop(lifted),
                            DockletGeometry.keyboardDock(dock, keyboardTop()).top,
                            cardState.stepProgress,
                        )
                        IntOffset(0, (lerp(lifted.top, open, cardState.clampedProgress) - dock.top).roundToPx())
                    },
                slots = slots,
                cardProgress = { cardState.clampedProgress },
                backdrop = backdrop,
            )
        }

        val cameraActive by remember(cardState) {
            derivedStateOf {
                cardState.step == DockletStep.Step1 && cardState.clampedProgress >= CAMERA_START_AT_OPENING_FRACTION
            }
        }
        DockletCard(
            state = cardState,
            frames = frames,
            backdrop = backdrop,
            closeLabel = stringResource(RCommon.string.docklet_close_search),
            lobeIconColor = { rememberScannerIconColor(animating = scannerTooltipVisible) },
            onOpenRequest = onOpenCard,
            onCloseRequest = onCloseCard,
            onKeyboardRequest = onKeyboardRequest,
            lobeAnchor = { ScannerTooltip(tooltipVisible = scannerTooltipVisible, onTooltipDismiss = onScannerTooltipDismiss) },
        ) {
            ScanPanel(
                modifier = Modifier.fillMaxSize(),
                step = if (cardState.contentStep == DockletStep.Step2) AddContactStep.Search else AddContactStep.Scan,
                cameraActive = cameraActive,
                keyboardRequested = keyboardRequested,
                onSearchFieldClick = { onCardStepChange(DockletStep.Step2) },
                onScanClick = { onCardStepChange(DockletStep.Step1) },
                onClose = onCloseCard,
                onScanHandled = onScanHandled,
            )
        }
    }
}

private fun DockletSlotTarget.icon() = when (this) {
    is DockletSlotTarget.Tab -> tab.icon()
    DockletSlotTarget.NetworkStatus -> NovaIcons.NetworkStatus
}

@Composable
private fun DockletSlotTarget.title() = when (this) {
    is DockletSlotTarget.Tab -> tab.title()
    DockletSlotTarget.NetworkStatus -> stringResource(RCommon.string.chain_health_panel_title)
}

@Composable
private fun ExpandablePanel(visible: Boolean, content: @Composable () -> Unit) {
    AnimatedVisibility(
        visible = visible,
        enter = expandVertically(expandFrom = Alignment.Bottom) + fadeIn(),
        exit = shrinkVertically(shrinkTowards = Alignment.Bottom) + fadeOut(),
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            PolkadotSurface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(PanelCornerRadius),
                color = PolkadotTheme.colors.bg.surface.container,
                border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary),
            ) {
                content()
            }
            VerticalSpacer { tiny }
        }
    }
}

@Composable
private fun OpenAppsRow(apps: ImmutableList<TabInfo>, onAppClick: (Long) -> Unit, onAppClose: (Long) -> Unit) {
    // The tab a long press opened the action menu for; only one menu is up at a time.
    var menuTabId by remember { mutableStateOf<Long?>(null) }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.medium),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        if (apps.isEmpty()) {
            // Same footprint as one row of cells, with a centered hint instead of products.
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(EmptyAppsRowHeight),
                contentAlignment = Alignment.Center,
            ) {
                NovaText(
                    text = stringResource(RCommon.string.tab_bar_no_open_products),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                )
            }
        } else {
            val rows = remember(apps) { apps.chunked(APPS_PER_ROW) }
            rows.forEach { rowApps ->
                Row(modifier = Modifier.fillMaxWidth()) {
                    rowApps.forEach { app ->
                        // The badge is wrapped so the menu popup anchors to the cell instead of becoming a
                        // zero-size child of the badge's spaced column.
                        Box(modifier = Modifier.weight(1f)) {
                            OpenAppBadge(
                                modifier = Modifier.combinedClickable(
                                    onClick = { onAppClick(app.id) },
                                    onLongClick = { menuTabId = app.id },
                                    indication = null,
                                    interactionSource = remember(app.id) { MutableInteractionSource() },
                                ),
                                title = app.title,
                                iconUrl = app.iconUrl,
                            )

                            if (menuTabId == app.id) {
                                AppActionMenu(
                                    onClose = {
                                        menuTabId = null
                                        onAppClose(app.id)
                                    },
                                    onDismiss = { menuTabId = null },
                                )
                            }
                        }
                    }
                    repeat(APPS_PER_ROW - rowApps.size) {
                        Spacer(modifier = Modifier.weight(1f))
                    }
                }
            }
        }
    }
}

// The long-press menu for an open product, styled like the chat action menu: a floating card above the
// pressed badge, dismissed by tapping outside it.
@Composable
private fun AppActionMenu(onClose: () -> Unit, onDismiss: () -> Unit) {
    val density = LocalDensity.current
    val gap = PolkadotTheme.spacings.small
    val screenMargin = PolkadotTheme.spacings.medium
    val positionProvider = remember(density, gap, screenMargin) {
        with(density) { AboveAnchorPositionProvider(gap.roundToPx(), screenMargin.roundToPx()) }
    }

    Popup(
        popupPositionProvider = positionProvider,
        onDismissRequest = onDismiss,
        properties = PopupProperties(focusable = true),
    ) {
        PolkadotSurface(
            shape = PolkadotTheme.shapes.large,
            color = PolkadotTheme.colors.bg.surface.container,
            border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
            shadowElevation = AppMenuShadowElevation,
        ) {
            Row(
                modifier = Modifier
                    .clickable(onClick = onClose)
                    .padding(
                        horizontal = PolkadotTheme.spacings.mediumIncreased,
                        vertical = PolkadotTheme.spacings.smallIncreased,
                    ),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                NovaIcon(
                    modifier = Modifier.size(AppMenuIconSize),
                    imageVector = NovaIcons.Trash,
                    tint = PolkadotTheme.colors.fg.primary,
                )

                HorizontalSpacer { smallIncreased }

                NovaText(
                    text = stringResource(RCommon.string.tab_bar_close_tab),
                    style = PolkadotTheme.typography.body.large,
                    color = PolkadotTheme.colors.fg.primary,
                )
            }
        }
    }
}

// Places the popup above its anchor and centred on it — the apps grid sits at the bottom of the screen, so
// there is never room below. Both axes are kept inside the window by [screenMarginPx].
private class AboveAnchorPositionProvider(
    private val gapPx: Int,
    private val screenMarginPx: Int,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val maxX = (windowSize.width - popupContentSize.width - screenMarginPx).coerceAtLeast(screenMarginPx)
        val x = (anchorBounds.center.x - popupContentSize.width / 2).coerceIn(screenMarginPx, maxX)
        val y = (anchorBounds.top - gapPx - popupContentSize.height).coerceAtLeast(screenMarginPx)

        return IntOffset(x, y)
    }
}

@Composable
private fun OpenAppBadge(
    modifier: Modifier,
    title: String,
    iconUrl: String?,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
    ) {
        // Show the product icon once it loads; until then (and if it has none or fails) show the first letter.
        var iconLoaded by remember(iconUrl) { mutableStateOf(false) }
        Box(
            modifier = Modifier
                .size(AppIconSize)
                .clip(CircleShape)
                .background(PolkadotTheme.colors.bg.surface.nested),
            contentAlignment = Alignment.Center,
        ) {
            if (!iconLoaded) {
                NovaText(
                    text = title.trim().firstOrNull()?.uppercase().orEmpty(),
                    style = PolkadotTheme.typography.title.small,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                )
            }
            if (iconUrl != null) {
                NovaAsyncImage(
                    model = iconUrl,
                    modifier = Modifier
                        .fillMaxSize()
                        .clip(CircleShape),
                    contentScale = ContentScale.Crop,
                    onSuccess = { iconLoaded = true },
                    onError = { iconLoaded = false },
                )
            }
        }
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = title,
            style = PolkadotTheme.typography.label.smallEmphasized,
            color = PolkadotTheme.colors.fg.secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
    }
}
