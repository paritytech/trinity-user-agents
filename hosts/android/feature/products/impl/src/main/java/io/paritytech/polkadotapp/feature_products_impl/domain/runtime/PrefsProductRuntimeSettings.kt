package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import android.content.SharedPreferences
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

class PrefsProductRuntimeSettings(
    private val prefs: SharedPreferences,
    private val isDebugBuild: Boolean,
) : ProductRuntimeSettings {
    override fun isTrUAPIRuntimeEnabled(): Boolean =
        isDebugBuild && prefs.getBoolean(KEY_TRUAPI_ENABLED, true)

    override fun setTrUAPIRuntimeEnabled(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_TRUAPI_ENABLED, enabled).apply()
    }

    override fun isWasmiWorkerRuntimeEnabled(): Boolean =
        isDebugBuild && prefs.getBoolean(KEY_WASMI_WORKER_ENABLED, false)

    override fun setWasmiWorkerRuntimeEnabled(enabled: Boolean) {
        prefs.edit().putBoolean(KEY_WASMI_WORKER_ENABLED, enabled).apply()
    }

    override fun isWasmiWorkerProduct(productId: ProductId): Boolean =
        isDebugBuild && prefs.getBoolean(productId.wasmiWorkerKey(), false)

    override fun setWasmiWorkerProduct(productId: ProductId, enabled: Boolean) {
        prefs.edit().putBoolean(productId.wasmiWorkerKey(), enabled).apply()
    }

    private fun ProductId.wasmiWorkerKey() = "$KEY_WASMI_WORKER_PRODUCT_PREFIX$value"

    private companion object {
        const val KEY_TRUAPI_ENABLED = "truapi_runtime_enabled"
        const val KEY_WASMI_WORKER_ENABLED = "wasmi_worker_runtime_enabled"
        const val KEY_WASMI_WORKER_PRODUCT_PREFIX = "wasmi_worker_product."
    }
}
