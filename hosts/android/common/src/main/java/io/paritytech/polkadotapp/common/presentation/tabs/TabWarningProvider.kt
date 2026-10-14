package io.paritytech.polkadotapp.common.presentation.tabs

import kotlinx.coroutines.flow.Flow

interface TabWarningProvider {
    val tab: BottomTab
    fun observeWarning(): Flow<Boolean>
}
