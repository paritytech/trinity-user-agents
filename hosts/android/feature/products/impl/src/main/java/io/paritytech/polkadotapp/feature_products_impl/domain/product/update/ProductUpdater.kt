package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.subscribeIsForeground
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUpdates
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.launch
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductUpdater @Inject constructor(
    private val dotNsUpdates: DotNsUpdates,
    private val productsInUse: ProductsInUse,
    private val dotNsTldProvider: DotNsTldProvider,
    private val resolveProductUseCase: ResolveProductUseCase,
    private val workerSupervisor: TrUAPIWorkerSupervisor,
    private val appLifecycleObserver: AppLifecycleObserver,
) {
    private val checkLocks = KeyedMutex<String>()

    fun observePendingNames(): Flow<Set<String>> = dotNsUpdates.observePendingUpdates()

    suspend fun check(productId: ProductId): Result<Unit> = fetchUpdates(mapOf(productId.value to namesOf(productId)))

    suspend fun checkAll(): Result<Unit> {
        val names = dotNsUpdates.namesOnDevice()
        return fetchUpdates(names.groupBy { productIdOf(it)?.value ?: it })
    }

    suspend fun applyPage(pageName: String) = applyNames(listOf(pageName))

    fun start(scope: CoroutineScope) {
        productsInUse.observe()
            .scan(emptySet<ProductId>() to emptySet<ProductId>()) { (previous, _), current -> current to (current - previous) }
            .onEach { (_, started) -> scope.checkEach(started) }
            .launchIn(scope)

        appLifecycleObserver.subscribeIsForeground()
            .filter { it }
            .onEach { scope.checkEach(productsInUse.observe().first()) }
            .launchIn(scope)

        combine(observePendingNames(), productsInUse.observeShownNames()) { pending, shown -> pending - shown }
            .onEach { names -> applyNames(names) }
            .launchIn(scope)
    }

    private fun CoroutineScope.checkEach(productIds: Set<ProductId>) {
        productIds.forEach { productId ->
            launch { check(productId).logFailure("Failed to check $productId for an update") }
        }
    }

    private suspend fun applyNames(names: Collection<String>) {
        val pending = observePendingNames().first()
        val pendingNames = names.filter { it in pending }
        if (pendingNames.isEmpty()) return

        dotNsUpdates.applyUpdates(pendingNames)

        pendingNames.groupBy { productIdOf(it) }.forEach { (productId, appliedNames) ->
            if (productId == null) return@forEach
            resolveProductUseCase.invalidate(productId)
            if (appliedNames.any { isWorkerName(productId, it) }) workerSupervisor.reload(productId)
        }
    }

    private fun isWorkerName(productId: ProductId, dotNsName: String): Boolean {
        return dotNsName == productId.value || ProductManifest.hostOf(productId, ExecutableKind.WORKER).getOrNull()?.value == dotNsName
    }

    private suspend fun fetchUpdates(namesByProduct: Map<String, List<String>>): Result<Unit> {
        val results = namesByProduct.entries.mapAsync { (product, names) ->
            checkLocks.withLock(product) { dotNsUpdates.fetchUpdates(names) }
        }
        val failure = results.firstNotNullOfOrNull { it.exceptionOrNull() }

        return if (failure != null && results.all { it.isFailure }) Result.failure(failure) else Result.success(Unit)
    }

    private suspend fun namesOf(productId: ProductId): List<String> {
        return dotNsUpdates.namesOnDevice().filter { productIdOf(it) == productId }
    }

    private suspend fun productIdOf(dotNsName: String): ProductId? {
        return dotNsTldProvider.getTld().getOrNull()?.let { tld -> productIdFromHost(dotNsName, tld).getOrNull() }
    }
}
