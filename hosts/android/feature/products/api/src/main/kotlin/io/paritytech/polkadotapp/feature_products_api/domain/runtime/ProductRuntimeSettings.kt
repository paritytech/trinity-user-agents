package io.paritytech.polkadotapp.feature_products_api.domain.runtime

/**
 * Selects which product host runtime new sessions use: the native JS-bridge
 * host or the TrUAPI Rust core. Read once per session creation, so flipping
 * it affects the next session, not live ones.
 */
interface ProductRuntimeSettings {
    /** Defaults to the TrUAPI core. Debug-only: release builds always run the native host. */
    fun isTrUAPIRuntimeEnabled(): Boolean

    fun setTrUAPIRuntimeEnabled(enabled: Boolean)

    /**
     * Runs TrUAPI product workers under the embedded wasmi sandbox instead of a hidden WebView.
     * Debug-only and off by default; read when a worker boots, so flipping it affects the next
     * boot. The worker executable must then be an ABI-compatible wasm module, not JavaScript.
     */
    fun isWasmiWorkerRuntimeEnabled(): Boolean

    fun setWasmiWorkerRuntimeEnabled(enabled: Boolean)
}
