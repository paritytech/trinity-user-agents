package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import io.paritytech.polkadotapp.app.root.presentation.root.RootNavBarViewModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel

/**
 * Host for the global navigation bar: resolves its own [RootNavBarViewModel] and renders the overlay.
 * The [navController]'s current entry is the "screen entered" signal the bar resets its posture on.
 */
@Composable
fun RootNavBarHost(navController: NavController, chainsHealth: ChainHealthIndicatorsModel) {
    val viewModel: RootNavBarViewModel = hiltViewModel()

    val hidden by viewModel.hidden.collectAsStateWithLifecycle()
    val forceShown by viewModel.forced.collectAsStateWithLifecycle()
    val currentEntry by navController.currentBackStackEntryFlow.collectAsStateWithLifecycle(initialValue = null)
    val currentTab by viewModel.currentTab.collectAsStateWithLifecycle()
    val tabWarnings by viewModel.tabWarnings.collectAsStateWithLifecycle()
    val openApps by viewModel.openApps.collectAsStateWithLifecycle()
    val tooltipVisible by viewModel.isScannerTooltipVisible.collectAsStateWithLifecycle()

    RootNavBarOverlay(
        hidden = hidden,
        forceShown = forceShown,
        screenKey = currentEntry?.id,
        currentTab = currentTab,
        tabWarnings = tabWarnings,
        openApps = openApps,
        chainsHealth = chainsHealth,
        scannerTooltipVisible = tooltipVisible,
        openScanPanelRequests = viewModel.scanPanelOpenRequests,
        onTabSelected = viewModel::onTabSelected,
        onScannerTooltipDismiss = viewModel::dismissScannerTooltip,
        onAppClick = viewModel::onAppClick,
        onAppClose = viewModel::onAppClose,
        onOffset = viewModel::setBarOffset,
        onBarHeight = viewModel::setBarHeight,
    )
}
