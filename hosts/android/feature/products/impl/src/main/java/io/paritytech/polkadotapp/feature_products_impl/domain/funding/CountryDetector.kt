package io.paritytech.polkadotapp.feature_products_impl.domain.funding

/** The country the user is likely in, as ISO 3166-1 alpha-2, for the core's funding quotes; `null` when nothing says. */
interface CountryDetector {
    suspend fun detect(): String?
}
