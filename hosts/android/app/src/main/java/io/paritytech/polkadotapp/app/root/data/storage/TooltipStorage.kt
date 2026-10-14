package io.paritytech.polkadotapp.app.root.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import javax.inject.Inject

enum class OneShotTooltip(val preferenceKey: String) {
    Scanner("scanner_tooltip_shown"),
    NetworkStatus("network_status_tooltip_shown"),
}

class TooltipStorage @Inject constructor(
    private val preferences: Preferences
) {
    fun wasShown(tooltip: OneShotTooltip): Boolean = preferences.getBoolean(tooltip.preferenceKey, false)

    fun markShown(tooltip: OneShotTooltip) {
        preferences.putBoolean(tooltip.preferenceKey, true)
    }
}
