package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUpdates
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductUpdater @Inject constructor(
    private val dotNsUpdates: DotNsUpdates,
    private val dotNsTldProvider: DotNsTldProvider,
    private val productsInUse: ProductsInUse,
    private val resolveProductUseCase: ResolveProductUseCase,
    private val workerSupervisor: TrUAPIWorkerSupervisor,
) {
    private val checkLocks = KeyedMutex<String>()

    private val foundUpdates = MutableSharedFlow<Set<String>>(extraBufferCapacity = FOUND_UPDATES_BUFFER_SIZE)

    fun observePendingNames(): Flow<Set<String>> = dotNsUpdates.observePendingUpdates()

    fun observeFoundUpdates(): Flow<Set<String>> = foundUpdates

    suspend fun check(productId: ProductId): Result<Unit> = namesOnDeviceByProduct().flatMap { namesByProduct ->
        fetchUpdates(namesByProduct.filterKeys { it == productId.value })
    }

    suspend fun checkAll(): Result<Unit> = namesOnDeviceByProduct().flatMap { namesByProduct -> fetchUpdates(namesByProduct) }

    suspend fun applyUpdates(dotNsNames: Collection<String>): Set<String> {
        val appliedNames = dotNsUpdates.applyUpdates(dotNsNames)
        val tld = dotNsTldProvider.getTldRetrying()

        appliedNames.groupBy { productIdFromHost(it, tld).getOrNull() }.forEach { (productId, productNames) ->
            if (productId == null) return@forEach
            resolveProductUseCase.invalidate(productId)
            if (productNames.any { isWorkerName(productId, it) }) workerSupervisor.reload(productId)
        }

        return appliedNames
    }

    suspend fun applyUpdateAndReloadPages(dotNsName: String): Boolean {
        val appliedNames = applyUpdates(listOf(dotNsName))
        appliedNames.forEach(productsInUse::reloadPagesShowing)

        return appliedNames.isNotEmpty()
    }

    private suspend fun namesOnDeviceByProduct(): Result<Map<String, List<String>>> = dotNsTldProvider.getTld().map { tld ->
        dotNsUpdates.namesOnDevice().groupBy { dotNsName -> productIdFromHost(dotNsName, tld).getOrNull()?.value ?: dotNsName }
    }

    private suspend fun fetchUpdates(namesByProduct: Map<String, List<String>>): Result<Unit> {
        val results = namesByProduct.entries
            .mapAsync { (product, productNames) -> checkLocks.withLock(product) { dotNsUpdates.fetchUpdates(productNames) } }

        val foundNames = results.flatMapTo(HashSet()) { it.getOrNull().orEmpty() }
        if (foundNames.isNotEmpty()) foundUpdates.emit(foundNames)

        return results.flattenResult().coerceToUnit()
    }

    private fun isWorkerName(productId: ProductId, dotNsName: String): Boolean {
        return dotNsName == productId.value || ProductManifest.hostOf(productId, ExecutableKind.WORKER).getOrNull()?.value == dotNsName
    }

    private companion object {
        const val FOUND_UPDATES_BUFFER_SIZE = 16
    }
}
