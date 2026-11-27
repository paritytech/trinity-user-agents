package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

data class ShownPage(val productId: ProductId, val dotNsName: String)

@Singleton
class ProductsInUse @Inject constructor() {
    private val productHolds = MutableStateFlow<Map<ProductId, Int>>(emptyMap())

    private val shownPages = MutableStateFlow<Map<TrackedPage, ShownPage>>(emptyMap())

    fun observe(): Flow<Set<ProductId>> {
        return combine(productHolds, shownPages) { holds, pages -> holds.keys + pages.values.map { it.productId } }
            .distinctUntilChanged()
    }

    fun observeShownPages(): Flow<List<ShownPage>> = shownPages.map { it.values.toList() }.distinctUntilChanged()

    fun observeShownNames(): Flow<Set<String>> {
        return shownPages.map { pages -> pages.values.mapTo(HashSet()) { it.dotNsName } }.distinctUntilChanged()
    }

    fun reloadPagesShowing(dotNsName: String) {
        shownPages.value.filterValues { it.dotNsName == dotNsName }.keys.forEach { it.reload() }
    }

    fun hold(productId: ProductId, scope: CoroutineScope) {
        acquire(productId)
        scope.invokeOnCompletion { release(productId) }
    }

    fun trackPage(pages: Flow<ShownPage?>, scope: CoroutineScope, reload: () -> Unit) {
        val trackedPage = TrackedPage(reload)

        pages
            .onEach { page -> shownPages.update { if (page == null) it - trackedPage else it + (trackedPage to page) } }
            .onCompletion { shownPages.update { it - trackedPage } }
            .launchIn(scope)
    }

    private fun acquire(productId: ProductId) {
        productHolds.update { it + (productId to (it[productId] ?: 0) + 1) }
    }

    private fun release(productId: ProductId) {
        productHolds.update { holds ->
            val left = (holds[productId] ?: 1) - 1
            if (left > 0) holds + (productId to left) else holds - productId
        }
    }

    private class TrackedPage(val reload: () -> Unit)
}
