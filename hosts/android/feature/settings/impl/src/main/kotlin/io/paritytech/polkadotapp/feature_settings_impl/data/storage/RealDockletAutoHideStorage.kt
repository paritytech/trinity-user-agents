package io.paritytech.polkadotapp.feature_settings_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_settings_api.data.storage.DockletAutoHideStorage
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealDockletAutoHideStorage @Inject constructor(
    private val preferences: Preferences,
) : DockletAutoHideStorage {
    override val enabled = MutableStateFlow(preferences.getBoolean(ENABLED_KEY, DEFAULT_ENABLED))

    override fun setEnabled(enabled: Boolean) {
        preferences.putBoolean(ENABLED_KEY, enabled)
        this.enabled.value = enabled
    }

    private companion object {
        const val ENABLED_KEY = "docklet_auto_hide_enabled"
        const val DEFAULT_ENABLED = true
    }
}
