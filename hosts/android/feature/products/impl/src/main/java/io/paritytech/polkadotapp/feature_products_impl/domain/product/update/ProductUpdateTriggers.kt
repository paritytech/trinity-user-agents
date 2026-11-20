package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.subscribeIsForeground
import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import javax.inject.Inject

class ProductUpdateTriggers @Inject constructor(
    private val productUpdater: ProductUpdater,
    private val productsInUse: ProductsInUse,
    private val appLifecycleObserver: AppLifecycleObserver,
) {
    fun start(scope: CoroutineScope) {
        productsInUse.observe()
            .scan(emptySet<ProductId>() to emptySet<ProductId>()) { (previous, _), current -> current to (current - previous) }
            .onEach { (_, started) -> scope.checkEach(started) }
            .launchIn(scope)

        appLifecycleObserver.subscribeIsForeground()
            .filter { it }
            .onEach { scope.checkEach(productsInUse.observe().first()) }
            .launchIn(scope)

        combine(productUpdater.observePendingNames(), productsInUse.observeShownNames()) { pending, shown -> pending - shown }
            .onEach { notShownNames -> productUpdater.applyUpdates(notShownNames) }
            .launchIn(scope)
    }

    private fun CoroutineScope.checkEach(productIds: Set<ProductId>) {
        productIds.forEach { productId ->
            launch { productUpdater.check(productId).logFailure("Failed to check $productId for an update") }
        }
    }
}
