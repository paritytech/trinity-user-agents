package io.paritytech.polkadotapp.common.presentation.tabs

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class BottomTabSelection @Inject constructor() {
    val currentTab: StateFlow<BottomTab>
        field = MutableStateFlow(BottomTab.CHATS)

    val reselectedTab: SharedFlow<BottomTab>
        field = MutableSharedFlow<BottomTab>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    fun select(tab: BottomTab) {
        if (tab in BottomTab.availableEntries) currentTab.value = tab
    }

    fun onTapped(tab: BottomTab) {
        if (currentTab.value == tab) reselectedTab.tryEmit(tab) else select(tab)
    }
}
