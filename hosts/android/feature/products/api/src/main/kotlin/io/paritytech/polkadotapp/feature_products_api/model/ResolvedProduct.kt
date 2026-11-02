package io.paritytech.polkadotapp.feature_products_api.model

/**
 * [description] and [executables] are in-memory only, re-fetched on every resolve. Only [product] is
 * persisted. A blank manifest description is null.
 */
data class ResolvedProduct(
    val product: Product,
    val description: String?,
    val executables: Executables,
)
