package io.paritytech.polkadotapp.app.root.presentation.root

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab

sealed interface StatusBarTitleModel {
    data class Tab(val tab: BottomTab) : StatusBarTitleModel

    data object Product : StatusBarTitleModel

    data object None : StatusBarTitleModel
}

fun statusBarTitleOf(destinationId: Int?, currentTab: BottomTab): StatusBarTitleModel = when (destinationId) {
    R.id.mainFragment -> StatusBarTitleModel.Tab(currentTab)
    R.id.spaBrowserFragment -> StatusBarTitleModel.Product
    else -> StatusBarTitleModel.None
}
