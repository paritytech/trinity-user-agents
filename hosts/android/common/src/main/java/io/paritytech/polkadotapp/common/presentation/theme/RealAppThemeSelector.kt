package io.paritytech.polkadotapp.common.presentation.theme

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.edit
import io.paritytech.polkadotapp.design.theme.AppThemeSelector
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject
import javax.inject.Singleton

private const val KEY_SELECTED_APP_THEME_ID = "selected_app_theme_id"

@Singleton
class RealAppThemeSelector @Inject constructor(
    private val preferences: Preferences
) : AppThemeSelector {
    override val selectedTheme = MutableStateFlow(
        PolkadotAppTheme.fromId(preferences.getString(KEY_SELECTED_APP_THEME_ID))
    )

    override fun select(theme: PolkadotAppTheme) {
        preferences.edit { putString(KEY_SELECTED_APP_THEME_ID, theme.id) }
        selectedTheme.value = theme
    }
}
