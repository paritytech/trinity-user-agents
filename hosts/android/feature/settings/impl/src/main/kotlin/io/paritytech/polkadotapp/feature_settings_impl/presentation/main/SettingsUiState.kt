package io.paritytech.polkadotapp.feature_settings_impl.presentation.main

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType

@Immutable
data class SettingsUiState(
    val debugMenuEnabled: Boolean,
    val linkedDevicesEnabled: Boolean,
    val productSettingsEnabled: Boolean,
    val isLanguageSettingsAvailable: Boolean,
    val selectedTheme: PolkadotAppTheme,
    /** Null until the stored preference has been read — the selector stays hidden rather than showing a guess. */
    val privacyMode: RecyclingStrategyType?,
    val isBackupMissing: Boolean = false,
    val hasBlockedUsers: Boolean = false,
)
