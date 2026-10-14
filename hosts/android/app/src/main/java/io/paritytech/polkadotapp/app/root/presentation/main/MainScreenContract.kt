package io.paritytech.polkadotapp.app.root.presentation.main

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import kotlinx.coroutines.flow.StateFlow

interface MainScreenContract {
    val tabWarnings: StateFlow<Map<BottomTab, Boolean>>

    val currentTab: StateFlow<BottomTab>

    fun selectTab(tab: BottomTab)
}
