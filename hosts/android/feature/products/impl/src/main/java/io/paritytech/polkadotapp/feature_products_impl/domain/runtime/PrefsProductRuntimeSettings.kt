package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings

class PrefsProductRuntimeSettings(
    private val prefs: SharedPreferences,
    private val isDebugBuild: Boolean,
) : ProductRuntimeSettings {
    override fun isTrUAPIRuntimeEnabled(): Boolean =
        isDebugBuild && prefs.getBoolean(KEY_TRUAPI_ENABLED, true)

    override fun setTrUAPIRuntimeEnabled(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_TRUAPI_ENABLED, enabled).apply()
    }

    private companion object {
        const val KEY_TRUAPI_ENABLED = "truapi_runtime_enabled"
    }
}
