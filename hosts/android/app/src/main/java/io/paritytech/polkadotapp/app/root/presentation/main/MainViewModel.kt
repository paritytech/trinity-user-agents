package io.paritytech.polkadotapp.app.root.presentation.main

import android.Manifest
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.app.root.domain.main.MainInteractor
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class MainViewModel @Inject constructor(
    private val navigationHolder: NavigationHolder,
    private val permissionAsker: PermissionAsker,
    private val interactor: MainInteractor
) : BaseViewModel() {
    // Selected tab lives in the shared NavigationHolder (so the root navigation bar drives it too).
    val currentTab: StateFlow<BottomTab> = navigationHolder.currentTab

    init {
        interactor.initialize()

        launch {
            permissionAsker.askPermission(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}
