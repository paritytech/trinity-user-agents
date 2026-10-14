package io.paritytech.polkadotapp.feature_products_api.domain.pocket

sealed class PocketRemoveError(message: String) : Throwable(message) {
    data object Privileged : PocketRemoveError("privileged cards cannot be removed")

    // Variants are singletons, so a captured trace would point at classloading, not the failure.
    override fun fillInStackTrace(): Throwable = this
}
