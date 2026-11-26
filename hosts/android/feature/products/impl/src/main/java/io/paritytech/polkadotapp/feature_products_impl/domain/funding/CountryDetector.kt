package io.paritytech.polkadotapp.feature_products_impl.domain.funding

interface CountryDetector {
    suspend fun detect(): String?
}
