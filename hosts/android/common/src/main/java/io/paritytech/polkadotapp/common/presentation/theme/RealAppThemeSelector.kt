package io.paritytech.polkadotapp.common.presentation.theme

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.edit
import io.paritytech.polkadotapp.design.theme.AppThemeSelector
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

private const val KEY_SELECTED_APP_THEME_ID = "selected_app_theme_id"

@Singleton
class RealAppThemeSelector @Inject constructor(
    private val preferences: Preferences
) : AppThemeSelector {
    override val selectedTheme: Flow<PolkadotAppTheme> = preferences
        .stringFlow(KEY_SELECTED_APP_THEME_ID)
        .map { id -> id?.let(PolkadotAppTheme.Companion::fromId) ?: PolkadotAppTheme.DEFAULT }

    override fun select(theme: PolkadotAppTheme) {
        preferences.edit { putString(KEY_SELECTED_APP_THEME_ID, theme.id) }
    }
}
