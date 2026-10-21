package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.updateTransition
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
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.exclude
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastForEach
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import io.paritytech.polkadotapp.app.root.presentation.main.compose.components.ScannerIconWithTooltip
import io.paritytech.polkadotapp.app.root.presentation.main.compose.icon
import io.paritytech.polkadotapp.app.root.presentation.main.compose.title
import io.paritytech.polkadotapp.app.root.presentation.root.compose.components.ScanPanel
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.NetworkStatus
import io.paritytech.polkadotapp.design.components.icon.vectors.TabsBox
import io.paritytech.polkadotapp.design.components.icon.vectors.TabsDigitBox
import io.paritytech.polkadotapp.design.components.icon.vectors.Trash
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.navigationbar.PolkadotNavigationBar
import io.paritytech.polkadotapp.design.components.navigationbar.PolkadotNavigationBarItem
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.modifyIf
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.ChainHealthPanel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.coroutines.flow.first
import io.paritytech.polkadotapp.common.R as RCommon

private const val APPS_PER_ROW = 5
private val AppIconSize = 24.dp

// Height of the empty-state block — roughly one row of app cells (icon + label), so the container looks
// the same size whether it shows one row of products or the "no products" hint.
private val EmptyAppsRowHeight = 44.dp

// Height of the center pill (scanner + open-tabs). Matches the tab row so the whole bar sits at 62dp
// (54 + the nav bar's 4dp top/bottom padding).
private val CenterPillHeight = 54.dp

// Corner radius shared by the bar (a pill at its height) and the products container — same rounding.
private val NavBarCornerRadius = 32.dp

private const val CAMERA_START_AT_OPENING_FRACTION = 0.3f

private val AppMenuIconSize = 20.dp
private val AppMenuShadowElevation = 8.dp

/**
 * The global navigation bar: the available tab buttons around a center pill that holds the scanner and,
 * beside it, the open-tabs button (a stacked-cards icon with the tab count — white while the apps grid is
 * expanded, otherwise the same muted colour as the scanner). Without the Browse tab there are no product
 * tabs to manage, so the pill keeps the scanner alone.
 *
 * With [FeatureOption.FULL_TAB_BAR] off the bar is reduced to icons: no item labels, and the scanner drops
 * the pill to sit as a bare icon — unless [FeatureOption.TAB_BAR_CONNECTIVITY_INDICATOR] is on, whose design
 * keeps the pill and the full width with the labels still off.
 */
@Composable
fun RootNavBar(
    modifier: Modifier = Modifier,
    currentTab: BottomTab,
    tabWarnings: ImmutableMap<BottomTab, Boolean>,
    apps: ImmutableList<TabInfo>,
    appsExpanded: Boolean,
    chainsHealth: ChainHealthIndicatorsModel,
    networkStatusExpanded: Boolean,
    scannerTooltipVisible: Boolean,
    scanExpanded: Boolean,
    onTabSelected: (BottomTab) -> Unit,
    onCountClicked: () -> Unit,
    onNetworkStatusClicked: () -> Unit,
    onAppClick: (Long) -> Unit,
    onAppClose: (Long) -> Unit,
    onScanClicked: () -> Unit,
    onScanHandled: (navigate: (() -> Unit)?) -> Unit,
    onScanDismiss: () -> Unit,
    onScannerTooltipDismiss: () -> Unit,
    onRestingHeightChange: (Dp) -> Unit,
) {
    val availableTabs = BottomTab.availableEntries
    val fullTabBar = FeatureOption.FULL_TAB_BAR.isEnabled
    val networkStatusItem = FeatureOption.TAB_BAR_CONNECTIVITY_INDICATOR.isEnabled
    val pillBar = fullTabBar || networkStatusItem
    val networkStatusUp = networkStatusItem && networkStatusExpanded
    val networkStatusSlot = availableTabs.size
    val selectedIndex = if (networkStatusUp) networkStatusSlot else availableTabs.indexOf(currentTab).coerceAtLeast(0)

    val scanPanel = updateTransition(targetState = scanExpanded, label = "ScanPanel")
    val scanPanelShown = scanPanel.currentState || scanPanel.targetState
    val focusManager = LocalFocusManager.current

    // The scan panel holds the username field. Its focus goes as soon as the panel starts closing, so the keyboard
    // leaves while the panel animates out and the bar, still padded above it, rides down with it.
    LaunchedEffect(scanExpanded) {
        if (!scanExpanded) focusManager.clearFocus()
    }

    val density = LocalDensity.current
    var tabRowHeight by remember { mutableStateOf(0.dp) }
    val topPadding = PolkadotTheme.spacings.small
    val bottomPadding = PolkadotTheme.spacings.small

    val navigationBarsBottom = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val currentOnRestingHeightChange by rememberUpdatedState(onRestingHeightChange)
    LaunchedEffect(tabRowHeight, navigationBarsBottom) {
        if (tabRowHeight > 0.dp) {
            currentOnRestingHeightChange(topPadding + tabRowHeight + bottomPadding + navigationBarsBottom)
        }
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = topPadding, bottom = bottomPadding)
            .navigationBarsPadding()
            .modifyIf(scanPanelShown) {
                windowInsetsPadding(WindowInsets.ime.exclude(WindowInsets(bottom = tabRowHeight + bottomPadding)))
            },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        ExpandablePanel(visible = appsExpanded) {
            OpenAppsRow(apps = apps, onAppClick = onAppClick, onAppClose = onAppClose)
        }
        if (networkStatusItem) {
            ExpandablePanel(visible = networkStatusExpanded) {
                ChainHealthPanel(model = chainsHealth)
            }
        }

        // Weighted so that on a short screen the panel, not the bar below it, gives up height.
        BarContainer(modifier = Modifier.weight(1f, fill = false)) {
            scanPanel.AnimatedVisibility(
                visible = { it },
                modifier = Modifier.weight(1f, fill = false),
                enter = expandVertically(expandFrom = Alignment.Bottom) + fadeIn(),
                exit = shrinkVertically(shrinkTowards = Alignment.Bottom) + fadeOut(),
            ) {
                var cameraActive by remember { mutableStateOf(false) }
                LaunchedEffect(scanExpanded) {
                    if (scanExpanded) {
                        snapshotFlow {
                            val playTime = scanPanel.playTimeNanos
                            scanPanel.currentState ||
                                (playTime > 0 && playTime >= scanPanel.totalDurationNanos * CAMERA_START_AT_OPENING_FRACTION)
                        }.first { it }
                        cameraActive = true
                    }
                }

                BackHandler(enabled = scanExpanded, onBack = onScanDismiss)

                val dragState = rememberScanPanelDragState(onDismiss = onScanDismiss)
                LaunchedEffect(scanExpanded) {
                    if (scanExpanded) dragState.reset()
                }

                val panelPadding = PolkadotTheme.spacings.small
                ScanPanel(
                    modifier = Modifier
                        .fillMaxWidth()
                        .dragToDismiss(dragState)
                        .padding(panelPadding),
                    scannerShape = RoundedCornerShape(NavBarCornerRadius - panelPadding),
                    cameraActive = cameraActive,
                    onScanHandled = onScanHandled,
                )
            }

            PolkadotNavigationBar(
                modifier = Modifier
                    .onSizeChanged { tabRowHeight = with(density) { it.height.toDp() } },
                selectedIndex = selectedIndex,
                itemCount = availableTabs.size + if (networkStatusItem) 1 else 0,
                shape = RoundedCornerShape(NavBarCornerRadius),
                color = Color.Transparent,
                border = null,
                fillWidth = pillBar,
                centerContent = {
                    if (pillBar) {
                        CenterPill(
                            scannerTooltipVisible = scannerTooltipVisible,
                            scanActive = scanExpanded,
                            onScanClicked = onScanClicked,
                            onScannerTooltipDismiss = onScannerTooltipDismiss,
                            tabsVisible = FeatureOption.BROWSE_TAB.isEnabled,
                            tabsCount = apps.size,
                            appsExpanded = appsExpanded,
                            onTabsClicked = onCountClicked,
                        )
                    } else {
                        ScannerButton(
                            scannerTooltipVisible = scannerTooltipVisible,
                            active = scanExpanded,
                            onScanClicked = onScanClicked,
                            onScannerTooltipDismiss = onScannerTooltipDismiss,
                            shape = PolkadotTheme.shapes.full,
                        )
                    }
                },
            ) {
                availableTabs.fastForEach { tab ->
                    PolkadotNavigationBarItem(
                        selected = tab == currentTab && !networkStatusUp,
                        onClick = { onTabSelected(tab) },
                        icon = tab.icon(),
                        label = if (fullTabBar) tab.title() else null,
                        hasNotification = tabWarnings[tab] == true,
                    )
                }
                if (networkStatusItem) {
                    PolkadotNavigationBarItem(
                        selected = networkStatusExpanded,
                        onClick = onNetworkStatusClicked,
                        label = null,
                    ) { contentColor ->
                        NovaIcon(
                            modifier = Modifier.fillMaxSize(),
                            imageVector = NovaIcons.NetworkStatus,
                            tint = contentColor,
                            contentDescription = stringResource(RCommon.string.chain_health_panel_title),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun BarContainer(modifier: Modifier, content: @Composable ColumnScope.() -> Unit) {
    PolkadotSurface(
        modifier = modifier,
        shape = RoundedCornerShape(NavBarCornerRadius),
        color = PolkadotTheme.colors.bg.surface.container,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary),
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, content = content)
    }
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
                shape = RoundedCornerShape(NavBarCornerRadius),
                color = PolkadotTheme.colors.bg.surface.container,
                border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary),
            ) {
                content()
            }
            VerticalSpacer { tiny }
        }
    }
}

// The center pill: scanner and open-tabs button sharing one rounded container, split by a thin divider.
@Composable
private fun CenterPill(
    scannerTooltipVisible: Boolean,
    scanActive: Boolean,
    onScanClicked: () -> Unit,
    onScannerTooltipDismiss: () -> Unit,
    tabsVisible: Boolean,
    tabsCount: Int,
    appsExpanded: Boolean,
    onTabsClicked: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier.height(CenterPillHeight),
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.nested,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
    ) {
        Row(
            modifier = Modifier.fillMaxHeight(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ScannerButton(
                scannerTooltipVisible = scannerTooltipVisible,
                active = scanActive,
                onScanClicked = onScanClicked,
                onScannerTooltipDismiss = onScannerTooltipDismiss,
                shape = RectangleShape,
            )

            if (tabsVisible) {
                Box(
                    modifier = Modifier
                        .width(PolkadotTheme.borders.default)
                        .padding(vertical = PolkadotTheme.spacings.smallIncreased)
                        .fillMaxHeight()
                        .background(PolkadotTheme.colors.stroke.secondary),
                )

                Box(
                    modifier = Modifier
                        .fillMaxHeight()
                        .clickable(onClick = onTabsClicked)
                        .padding(horizontal = PolkadotTheme.spacings.medium),
                    contentAlignment = Alignment.Center,
                ) {
                    OpenTabsIcon(count = tabsCount, active = appsExpanded)
                }
            }
        }
    }
}

// The scanner tap target, sized to the center pill so the bar keeps its height whether the pill is drawn
// around it or the icon stands alone. [shape] bounds the press ripple: inside the pill the surface already
// clips it, but standing alone the button has to round it off itself or the ripple comes out square.
@Composable
private fun ScannerButton(
    scannerTooltipVisible: Boolean,
    active: Boolean,
    onScanClicked: () -> Unit,
    onScannerTooltipDismiss: () -> Unit,
    shape: Shape,
) {
    Box(
        modifier = Modifier
            .size(CenterPillHeight)
            .clip(shape)
            .clickable(onClick = onScanClicked),
        contentAlignment = Alignment.Center,
    ) {
        ScannerIconWithTooltip(
            tooltipVisible = scannerTooltipVisible,
            active = active,
            onTooltipDismiss = onScannerTooltipDismiss,
        )
    }
}

// The stacked-tabs icon from the design: a front card ([NovaIcons.TabsDigitBox]) carrying the tab count,
// with a second card ([NovaIcons.TabsBox]) peeking behind it. White while [active] (apps grid expanded),
// otherwise the muted tab-secondary colour.
@Composable
private fun OpenTabsIcon(count: Int, active: Boolean) {
    val color by animateColorAsState(
        targetValue = if (active) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.secondary,
        label = "OpenTabsIconColor",
    )
    Box(modifier = Modifier.size(24.dp)) {
        NovaIcon(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .size(width = 18.dp, height = 17.dp),
            imageVector = NovaIcons.TabsBox,
            tint = color,
        )

        Box(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .size(19.dp),
            contentAlignment = Alignment.Center,
        ) {
            NovaIcon(
                modifier = Modifier.fillMaxSize(),
                imageVector = NovaIcons.TabsDigitBox,
                tint = color,
            )
            NovaText(
                text = count.toString(),
                style = PolkadotTheme.typography.label.smallEmphasized,
                color = color,
                maxLines = 1,
            )
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
