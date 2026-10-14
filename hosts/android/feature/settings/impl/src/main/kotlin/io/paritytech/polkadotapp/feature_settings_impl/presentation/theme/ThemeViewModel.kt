package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.design.theme.AppThemeSelector
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.models.ThemeUiState
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class ThemeViewModel @Inject constructor(
    private val router: SettingsRouter,
    private val appThemeSelector: AppThemeSelector
) : BaseViewModel() {
    private val availableThemes = PolkadotAppTheme.entries
        .sortedByDescending { it == PolkadotAppTheme.DEFAULT }
        .toImmutableList()

    val state: StateFlow<ThemeUiState> = appThemeSelector.selectedTheme
        .map { selected ->
            ThemeUiState(
                selectedTheme = selected,
                availableThemes = availableThemes
            )
        }
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = ThemeUiState(
                selectedTheme = PolkadotAppTheme.DEFAULT,
                availableThemes = availableThemes
            )
        )

    fun onThemeSelected(theme: PolkadotAppTheme) {
        if (theme == state.value.selectedTheme) return
        appThemeSelector.select(theme)
    }

    fun onBackClick() {
        router.back()
    }

    fun confirm() {
        router.openClaimUsername()
    }
}
