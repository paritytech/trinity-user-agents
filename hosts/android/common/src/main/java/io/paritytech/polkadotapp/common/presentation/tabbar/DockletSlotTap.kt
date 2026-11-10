package io.paritytech.polkadotapp.common.presentation.tabbar

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab

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

fun dockletHighlightedSlot(currentTab: BottomTab, networkStatusExpanded: Boolean): DockletSlotTarget =
    if (networkStatusExpanded) DockletSlotTarget.NetworkStatus else DockletSlotTarget.Tab(currentTab)
