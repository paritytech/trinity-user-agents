package io.paritytech.polkadotapp.feature_products_api.domain.error

/**
 * Product-resolution failures, discriminated by the UI to pick the message it shows. Variants stay
 * payload-free because this reaches Compose state: a nested [Throwable] would compare by identity
 * and defeat recomposition skipping.
 */
sealed class ProductResolutionError(message: String) : Throwable(message) {
    /** Permanent until the publisher fixes it — a retry re-reads the same bytes. */
    data object MalformedManifest : ProductResolutionError("product manifest is malformed")

    data object Unknown : ProductResolutionError("product resolution failed")

    // Variants are singletons, so a captured trace would point at classloading, not the failure.
    override fun fillInStackTrace(): Throwable = this
}
