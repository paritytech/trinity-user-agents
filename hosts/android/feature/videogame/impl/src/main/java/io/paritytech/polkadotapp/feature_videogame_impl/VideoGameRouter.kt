package io.paritytech.polkadotapp.feature_videogame_impl

import uniffi.truapi.ProductId

interface VideoGameRouter {
    suspend fun openGameProduct(productId: ProductId)
}
