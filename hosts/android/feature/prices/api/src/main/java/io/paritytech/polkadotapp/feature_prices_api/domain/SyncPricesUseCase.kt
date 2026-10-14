package io.paritytech.polkadotapp.feature_prices_api.domain

interface SyncPricesUseCase {
    suspend fun syncPrices(): Result<Unit>
}
