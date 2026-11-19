package io.paritytech.polkadotapp.feature_settings_api.data.storage

import kotlinx.coroutines.flow.StateFlow

/**
 * The user's Auto-hide docklet setting, on until they turn it off.
 */
interface DockletAutoHideStorage {
    val enabled: StateFlow<Boolean>

    fun setEnabled(enabled: Boolean)
}
