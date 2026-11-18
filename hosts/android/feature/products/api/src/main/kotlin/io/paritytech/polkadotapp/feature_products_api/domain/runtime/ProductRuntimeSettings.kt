package io.paritytech.polkadotapp.feature_products_api.domain.runtime

import uniffi.truapi.ProductSource

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
     * Whether a session for a product run from [source] uses the TrUAPI core. A product served from a development
     * server always does, since only the core gives it its id; any other follows the toggle.
     */
    fun isTrUAPIRuntimeEnabledFor(source: ProductSource?): Boolean =
        source is ProductSource.DevServer || isTrUAPIRuntimeEnabled()
}
