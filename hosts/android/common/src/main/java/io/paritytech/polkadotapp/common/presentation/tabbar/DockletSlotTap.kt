package io.paritytech.polkadotapp.common.presentation.tabbar

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.design.components.docklet.DockletSlotHighlight

sealed interface DockletSlotTarget {
    data class Tab(val tab: BottomTab) : DockletSlotTarget
    data object NetworkStatus : DockletSlotTarget
}

sealed interface DockletSlotAction {
    data object CloseCard : DockletSlotAction
    data object ToggleApps : DockletSlotAction
    data object ToggleNetworkStatus : DockletSlotAction
    data class SelectTab(val tab: BottomTab) : DockletSlotAction
}

fun dockletSlotTap(
    target: DockletSlotTarget,
    currentTab: BottomTab,
    onMain: Boolean,
    cardOpen: Boolean,
): DockletSlotAction = when {
    cardOpen -> DockletSlotAction.CloseCard
    else -> when (target) {
        DockletSlotTarget.NetworkStatus -> DockletSlotAction.ToggleNetworkStatus
        is DockletSlotTarget.Tab -> if (target.tab == BottomTab.EXPLORE && currentTab == BottomTab.EXPLORE && onMain) {
            DockletSlotAction.ToggleApps
        } else {
            DockletSlotAction.SelectTab(target.tab)
        }
    }
}

fun dockletSlotHighlight(target: DockletSlotTarget, currentTab: BottomTab, networkStatusExpanded: Boolean) = when {
    target == DockletSlotTarget.Tab(currentTab) -> DockletSlotHighlight.Selected
    target == DockletSlotTarget.NetworkStatus && networkStatusExpanded -> DockletSlotHighlight.Emphasized
    else -> DockletSlotHighlight.None
}
