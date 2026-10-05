package io.paritytech.polkadotapp.feature_products_api.domain.runtime

/**
 * Selects which product host runtime new sessions use: the native JS-bridge
 * host or the TrUAPI Rust core. Read once at process startup; changing it takes effect after restarting the app.
 */
interface ProductRuntimeSettings {
    /** Defaults to the TrUAPI core. Debug-only: release builds always run the native host. */
    fun isTrUAPIRuntimeEnabled(): Boolean

    fun isTrUAPIRuntimeEnabledOnNextLaunch(): Boolean

    fun setTrUAPIRuntimeEnabled(enabled: Boolean)
}
