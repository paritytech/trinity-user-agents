package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUpdates
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductUpdater @Inject constructor(
    private val dotNsUpdates: DotNsUpdates,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    private val checkLocks = KeyedMutex<String>()

    suspend fun check(productId: ProductId): Result<Unit> = namesOnDeviceByProduct().flatMap { namesByProduct ->
        fetchUpdates(namesByProduct.filterKeys { it == productId.value })
    }

    suspend fun checkAll(): Result<Unit> = namesOnDeviceByProduct().flatMap { namesByProduct -> fetchUpdates(namesByProduct) }

    private suspend fun namesOnDeviceByProduct(): Result<Map<String, List<String>>> = dotNsTldProvider.getTld().map { tld ->
        dotNsUpdates.namesOnDevice().groupBy { dotNsName -> productIdFromHost(dotNsName, tld).getOrNull()?.value ?: dotNsName }
    }

    private suspend fun fetchUpdates(namesByProduct: Map<String, List<String>>): Result<Unit> {
        return namesByProduct.entries
            .mapAsync { (product, productNames) -> checkLocks.withLock(product) { dotNsUpdates.fetchUpdates(productNames) } }
            .flattenResult()
            .coerceToUnit()
    }
}
