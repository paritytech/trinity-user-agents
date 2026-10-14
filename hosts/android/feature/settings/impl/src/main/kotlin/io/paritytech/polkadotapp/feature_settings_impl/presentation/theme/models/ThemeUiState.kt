package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class ThemeUiState(
    val selectedTheme: PolkadotAppTheme,
    val availableThemes: ImmutableList<PolkadotAppTheme>
)
