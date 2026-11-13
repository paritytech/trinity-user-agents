package io.paritytech.polkadotapp.app.root.presentation.root.compose

import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import io.paritytech.polkadotapp.app.root.presentation.root.DockletInput
import io.paritytech.polkadotapp.app.root.presentation.root.RootNavBarViewModel
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import kotlinx.coroutines.flow.Flow

/**
 * Host for the global navigation bar: resolves its own [RootNavBarViewModel] and renders the overlay.
 * The [navController]'s current entry is the "screen entered" signal the bar resets its posture on.
 */
@Composable
fun RootNavBarHost(
    navController: NavController,
    chainsHealth: ChainHealthIndicatorsModel,
    dockletInputs: Flow<DockletInput>,
    backdropSource: View,
) {
    val viewModel: RootNavBarViewModel = hiltViewModel()

    val barMode by viewModel.barMode.collectAsStateWithLifecycle()
    val currentEntry by navController.currentBackStackEntryFlow.collectAsStateWithLifecycle(initialValue = null)
    val currentTab by viewModel.currentTab.collectAsStateWithLifecycle()
    val tabWarnings by viewModel.tabWarnings.collectAsStateWithLifecycle()
    val openApps by viewModel.openApps.collectAsStateWithLifecycle()
    val tooltipVisible by viewModel.isScannerTooltipVisible.collectAsStateWithLifecycle()
    val dockletTip by viewModel.dockletTip.collectAsStateWithLifecycle()

    RootNavBarOverlay(
        barMode = barMode,
        screenKey = currentEntry?.id,
        currentTab = currentTab,
        tabWarnings = tabWarnings,
        openApps = openApps,
        chainsHealth = chainsHealth,
        scannerTooltipVisible = tooltipVisible,
        openScanPanelRequests = viewModel.scanPanelOpenRequests,
        dockletTip = dockletTip,
        dockletInputs = dockletInputs,
        backdropSource = backdropSource,
        onTabSelected = viewModel::onTabSelected,
        onScannerTooltipDismiss = viewModel::dismissScannerTooltip,
        onDockletHidden = viewModel::onDockletHidden,
        takeSessionPeek = viewModel::takeSessionPeek,
        onDockletTipDismiss = viewModel::dismissDockletTip,
        onAppClick = viewModel::onAppClick,
        onAppClose = viewModel::onAppClose,
        onOffset = viewModel::setBarOffset,
        onBarHeight = viewModel::setBarHeight,
        onBarHiddenFraction = viewModel::setBarHiddenFraction,
    )
}
