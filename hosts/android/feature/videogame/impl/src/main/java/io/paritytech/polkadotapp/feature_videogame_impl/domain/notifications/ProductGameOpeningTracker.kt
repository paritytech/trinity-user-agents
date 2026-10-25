package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductGameOpeningTracker @Inject constructor() {
    private val openingCounts = MutableStateFlow<Map<ProductId, Int>>(emptyMap())

    val openingProducts = openingCounts.asStateFlow()

    suspend fun <T> track(productId: ProductId, block: suspend () -> T): T {
        openingCounts.update { counts -> counts + (productId to counts.getOrDefault(productId, 0) + 1) }
        return try {
            block()
        } finally {
            openingCounts.update { counts ->
                val remaining = counts.getValue(productId) - 1
                if (remaining == 0) counts - productId else counts + (productId to remaining)
            }
        }
    }
}
