package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

/** The products with a page or a worker running, as the WebViews running them report. */
@Singleton
class ProductsInUse @Inject constructor() {
    private val holds = MutableStateFlow<Map<ProductId, Int>>(emptyMap())

    fun observe(): Flow<Set<ProductId>> = holds.map { it.keys }.distinctUntilChanged()

    /** Holds [productId] in use until [scope] ends. */
    fun hold(productId: ProductId, scope: CoroutineScope) {
        acquire(productId)
        scope.invokeOnCompletion { release(productId) }
    }

    /** Holds the latest of [productIds] in use, until the next one replaces it or [scope] ends. */
    fun track(productIds: Flow<ProductId?>, scope: CoroutineScope) {
        var held: ProductId? = null

        productIds
            .distinctUntilChanged()
            .onEach { next ->
                held?.let(::release)
                next?.let(::acquire)
                held = next
            }
            .onCompletion { held?.let(::release) }
            .launchIn(scope)
    }

    private fun acquire(productId: ProductId) {
        holds.update { it + (productId to (it[productId] ?: 0) + 1) }
    }

    private fun release(productId: ProductId) {
        holds.update { holds ->
            val left = (holds[productId] ?: 1) - 1
            if (left > 0) holds + (productId to left) else holds - productId
        }
    }
}
