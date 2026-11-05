package io.paritytech.polkadotapp.feature_products_api.domain

/** Full product urls, TLD included and scheme optional, e.g. `getcash.dot/onramp`. */
data class FundingConfig(
    val onrampUrl: String,
    val offrampUrl: String,
)
