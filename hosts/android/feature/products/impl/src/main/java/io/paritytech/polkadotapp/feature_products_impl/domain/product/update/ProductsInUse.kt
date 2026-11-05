package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

data class ShownPage(val productId: ProductId, val dotNsName: String?)

@Singleton
class ProductsInUse @Inject constructor() {
    private val productHolds = MutableStateFlow<Map<ProductId, Int>>(emptyMap())

    private val shownNameHolds = MutableStateFlow<Map<String, Int>>(emptyMap())

    fun observe(): Flow<Set<ProductId>> = productHolds.observeKeys()

    fun observeShownNames(): Flow<Set<String>> = shownNameHolds.observeKeys()

    fun hold(productId: ProductId, scope: CoroutineScope) {
        productHolds.acquire(productId)
        scope.invokeOnCompletion { productHolds.release(productId) }
    }

    fun trackPage(pages: Flow<ShownPage?>, scope: CoroutineScope) {
        var held: ShownPage? = null

        pages
            .distinctUntilChanged()
            .onEach { next ->
                held?.let(::release)
                next?.let(::acquire)
                held = next
            }
            .onCompletion { held?.let(::release) }
            .launchIn(scope)
    }

    private fun acquire(page: ShownPage) {
        productHolds.acquire(page.productId)
        page.dotNsName?.let(shownNameHolds::acquire)
    }

    private fun release(page: ShownPage) {
        productHolds.release(page.productId)
        page.dotNsName?.let(shownNameHolds::release)
    }
}

private fun <K> MutableStateFlow<Map<K, Int>>.observeKeys(): Flow<Set<K>> = map { it.keys }.distinctUntilChanged()

private fun <K> MutableStateFlow<Map<K, Int>>.acquire(key: K) {
    update { it + (key to (it[key] ?: 0) + 1) }
}

private fun <K> MutableStateFlow<Map<K, Int>>.release(key: K) {
    update { holds ->
        val left = (holds[key] ?: 1) - 1
        if (left > 0) holds + (key to left) else holds - key
    }
}
