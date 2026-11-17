package io.paritytech.polkadotapp.feature_settings_impl.data.repository

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_settings_api.domain.docklet.DockletAutoHideSettings
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealDockletAutoHideSettings @Inject constructor(
    private val preferences: Preferences,
) : DockletAutoHideSettings {
    override fun enabledFlow(): Flow<Boolean> = preferences.booleanFlow(KEY_ENABLED, DockletAutoHideSettings.DEFAULT_ENABLED)

    override fun setEnabled(enabled: Boolean) {
        preferences.putBoolean(KEY_ENABLED, enabled)
    }

    private companion object {
        const val KEY_ENABLED = "docklet_auto_hide_enabled"
    }
}
