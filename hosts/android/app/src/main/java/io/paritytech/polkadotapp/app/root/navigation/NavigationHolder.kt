package io.paritytech.polkadotapp.app.root.navigation

import androidx.navigation.NavController
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject

class NavigationHolder @Inject constructor(val contextManager: ContextManager) {
    var navController: NavController? = null
        private set

    val currentTab: StateFlow<BottomTab>
        field = MutableStateFlow(BottomTab.CHATS)

    fun attach(navController: NavController) {
        this.navController = navController
    }

    fun detach() {
        navController = null
    }

    fun requestTab(tab: BottomTab) {
        if (tab !in BottomTab.availableEntries) return

        currentTab.value = tab
    }

    fun finishApp() {
        contextManager.getActivity()?.finish()
    }
}

fun NavigationHolder.executeBack() {
    val popped = navController!!.popBackStack()

    if (!popped) {
        contextManager.getActivity()!!.finish()
    }
}
