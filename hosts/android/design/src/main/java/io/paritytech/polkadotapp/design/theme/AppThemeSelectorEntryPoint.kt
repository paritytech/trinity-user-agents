package io.paritytech.polkadotapp.design.theme

import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent

@EntryPoint
@InstallIn(SingletonComponent::class)
interface AppThemeSelectorEntryPoint {
    fun appThemeSelector(): AppThemeSelector

    fun systemBarsAppearanceStateHolder(): SystemBarsAppearanceStateHolder
}
