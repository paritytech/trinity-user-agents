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

    private val shownPages = MutableStateFlow<Map<TrackedPage, ShownPage>>(emptyMap())

    fun observe(): Flow<Set<ProductId>> = productHolds.map { it.keys }.distinctUntilChanged()

    fun observeShownPages(): Flow<List<ShownPage>> = shownPages.map { it.values.toList() }.distinctUntilChanged()

    fun observeShownNames(): Flow<Set<String>> {
        return shownPages.map { pages -> pages.values.mapNotNullTo(HashSet()) { it.dotNsName } }.distinctUntilChanged()
    }

    fun reloadPagesShowing(dotNsName: String) {
        shownPages.value.filterValues { it.dotNsName == dotNsName }.keys.forEach { it.reload() }
    }

    fun hold(productId: ProductId, scope: CoroutineScope) {
        productHolds.acquire(productId)
        scope.invokeOnCompletion { productHolds.release(productId) }
    }

    fun trackPage(pages: Flow<ShownPage?>, scope: CoroutineScope, reload: () -> Unit) {
        val trackedPage = TrackedPage(reload)
        var held: ShownPage? = null

        pages
            .distinctUntilChanged()
            .onEach { next ->
                held?.let { productHolds.release(it.productId) }
                next?.let { productHolds.acquire(it.productId) }
                shownPages.update { if (next == null) it - trackedPage else it + (trackedPage to next) }
                held = next
            }
            .onCompletion {
                held?.let { productHolds.release(it.productId) }
                shownPages.update { it - trackedPage }
            }
            .launchIn(scope)
    }

    private class TrackedPage(val reload: () -> Unit)
}

private fun <K> MutableStateFlow<Map<K, Int>>.acquire(key: K) {
    update { it + (key to (it[key] ?: 0) + 1) }
}

private fun <K> MutableStateFlow<Map<K, Int>>.release(key: K) {
    update { holds ->
        val left = (holds[key] ?: 1) - 1
        if (left > 0) holds + (key to left) else holds - key
    }
}
