package io.paritytech.polkadotapp.feature_products_api.model

/** A product's published executables. Any kind can be absent — legacy products have APP only. */
data class Executables(
    val app: ProductExecutable.App?,
    val widget: ProductExecutable.Widget?,
    val worker: ProductExecutable.Worker?,
)
