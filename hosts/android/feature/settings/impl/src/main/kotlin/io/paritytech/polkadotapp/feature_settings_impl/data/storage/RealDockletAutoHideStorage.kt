package io.paritytech.polkadotapp.feature_settings_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_settings_api.data.storage.DockletAutoHideStorage
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealDockletAutoHideStorage @Inject constructor(
    private val preferences: Preferences,
) : DockletAutoHideStorage {
    override fun isEnabled(): Boolean = preferences.getBoolean(ENABLED_KEY, DEFAULT_ENABLED)

    override fun observeEnabled(): Flow<Boolean> = preferences.booleanFlow(ENABLED_KEY, DEFAULT_ENABLED)

    override fun setEnabled(enabled: Boolean) {
        preferences.putBoolean(ENABLED_KEY, enabled)
    }

    private companion object {
        const val ENABLED_KEY = "docklet_auto_hide_enabled"
        const val DEFAULT_ENABLED = true
    }
}
