package io.paritytech.polkadotapp.feature_settings_api.data.storage

import kotlinx.coroutines.flow.StateFlow

/**
 * The user's Auto-hide docklet setting, off until they turn it on.
 */
interface DockletAutoHideStorage {
    val enabled: StateFlow<Boolean>

    fun setEnabled(enabled: Boolean)
}
