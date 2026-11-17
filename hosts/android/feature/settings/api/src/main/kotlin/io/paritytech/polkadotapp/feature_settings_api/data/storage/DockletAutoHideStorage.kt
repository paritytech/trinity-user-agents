package io.paritytech.polkadotapp.feature_settings_api.data.storage

import kotlinx.coroutines.flow.Flow

/**
 * Whether the Docklet tucks itself away after an outside touch or a few idle seconds; on until set.
 * When off, it stays on screen until the user swipes it to the right.
 * [observeEnabled] emits the current value on subscribe and again on every change.
 */
interface DockletAutoHideStorage {
    fun isEnabled(): Boolean

    fun observeEnabled(): Flow<Boolean>

    fun setEnabled(enabled: Boolean)
}
