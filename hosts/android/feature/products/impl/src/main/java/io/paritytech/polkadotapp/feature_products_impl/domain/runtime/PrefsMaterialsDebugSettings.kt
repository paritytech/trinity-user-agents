package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsDebugSettings
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsOverride

class PrefsMaterialsDebugSettings(
    private val prefs: SharedPreferences,
    private val debugToolsEnabled: Boolean,
) : MaterialsDebugSettings {
    // An unknown stored value, say from a later build, reads as no override rather than failing the runtime boot.
    override fun override(): MaterialsOverride {
        if (!debugToolsEnabled) return MaterialsOverride.REMOTE_CONFIG

        val stored = prefs.getString(KEY_MATERIALS_OVERRIDE, null)
        return MaterialsOverride.entries.firstOrNull { it.name == stored } ?: MaterialsOverride.REMOTE_CONFIG
    }

    override fun setOverride(override: MaterialsOverride) {
        prefs.edit().putString(KEY_MATERIALS_OVERRIDE, override.name).apply()
    }

    private companion object {
        const val KEY_MATERIALS_OVERRIDE = "materials_override"
    }
}
