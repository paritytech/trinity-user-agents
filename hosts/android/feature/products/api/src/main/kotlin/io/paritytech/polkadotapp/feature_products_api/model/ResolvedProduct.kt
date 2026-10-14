package io.paritytech.polkadotapp.feature_products_api.model

/** [executables] are in-memory only — re-fetched on every resolve. Only [product] is persisted. */
data class ResolvedProduct(
    val product: Product,
    val executables: Executables,
)
