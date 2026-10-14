package io.paritytech.polkadotapp.app.root.presentation.root

import androidx.compose.ui.unit.Dp
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.app.root.domain.main.MainInteractor
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.app.root.presentation.main.BottomNavHeightProvider
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarOffsetHolder
import io.paritytech.polkadotapp.common.presentation.tabbar.TabBarVisibilityHolder
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/**
 * State + actions for the global navigation bar (shown on every screen).
 */
@HiltViewModel
class RootNavBarViewModel @Inject constructor(
    private val navigationHolder: NavigationHolder,
    private val rootRouter: RootRouter,
    private val interactor: MainInteractor,
    private val productSessionController: ProductSessionController,
    private val tabBarVisibilityHolder: TabBarVisibilityHolder,
    private val tabBarOffsetHolder: TabBarOffsetHolder,
    private val bottomNavHeightProvider: BottomNavHeightProvider,
    scanPanelRequests: ScanPanelRequests,
) : BaseViewModel() {
    val currentTab: StateFlow<BottomTab> = navigationHolder.currentTab
    val hidden: StateFlow<Boolean> = tabBarVisibilityHolder.hidden
    val forced: StateFlow<Boolean> = tabBarVisibilityHolder.forced
    val openApps: StateFlow<ImmutableList<TabInfo>> = productSessionController.openTabs
        .map { it.toImmutableList() }
        .stateInBackground(started = SharingStarted.Eagerly, initialValue = persistentListOf())

    val tabWarnings: StateFlow<ImmutableMap<BottomTab, Boolean>> = interactor.observeTabWarnings()
        .map { it.toImmutableMap() }
        .stateInBackground(started = SharingStarted.Eagerly, initialValue = persistentMapOf())

    val isScannerTooltipVisible: StateFlow<Boolean>
        field = MutableStateFlow(interactor.shouldShowScannerTooltip())

    val scanPanelOpenRequests: SharedFlow<Unit> = scanPanelRequests.openRequests

    // Selecting a tab always lands on Main (bringing the user there if needed).
    fun onTabSelected(tab: BottomTab) {
        navigationHolder.requestTab(tab)
        rootRouter.openMain()
    }

    fun onAppClick(tabId: Long) {
        productSessionController.selectTab(tabId)
        rootRouter.openActiveProduct()
    }

    fun onAppClose(tabId: Long) {
        productSessionController.closeTab(tabId)
    }

    fun setBarOffset(value: Dp) {
        tabBarOffsetHolder.setOffset(value)
    }

    fun setBarHeight(value: Dp) {
        bottomNavHeightProvider.set(value)
    }

    fun dismissScannerTooltip() {
        if (!isScannerTooltipVisible.value) return
        isScannerTooltipVisible.value = false
        interactor.markScannerTooltipShown()
    }
}
