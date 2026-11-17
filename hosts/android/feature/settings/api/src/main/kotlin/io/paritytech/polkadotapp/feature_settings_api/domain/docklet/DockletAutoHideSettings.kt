package io.paritytech.polkadotapp.feature_settings_api.domain.docklet

import kotlinx.coroutines.flow.Flow

/**
 * Whether the Docklet tucks itself away after an outside touch or a few idle seconds; [DEFAULT_ENABLED] until set.
 * When off, it stays on screen until the user swipes it to the right.
 * [enabledFlow] emits the current value on subscribe and again on every change.
 */
interface DockletAutoHideSettings {
    fun enabledFlow(): Flow<Boolean>

    fun setEnabled(enabled: Boolean)

    companion object {
        const val DEFAULT_ENABLED = true
    }
}
