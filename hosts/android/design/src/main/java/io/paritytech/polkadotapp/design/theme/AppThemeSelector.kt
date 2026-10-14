package io.paritytech.polkadotapp.design.theme

import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import kotlinx.coroutines.flow.Flow

interface AppThemeSelector {
    val selectedTheme: Flow<PolkadotAppTheme>

    fun select(theme: PolkadotAppTheme)
}
