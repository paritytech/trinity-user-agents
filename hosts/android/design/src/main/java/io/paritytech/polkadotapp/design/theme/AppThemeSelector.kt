package io.paritytech.polkadotapp.design.theme

import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import kotlinx.coroutines.flow.StateFlow

interface AppThemeSelector {
    val selectedTheme: StateFlow<PolkadotAppTheme>

    fun select(theme: PolkadotAppTheme)
}
