package io.paritytech.polkadotapp.feature_products_api.model.signing

interface SigningRouter {
    suspend fun openSignTransaction()
}
