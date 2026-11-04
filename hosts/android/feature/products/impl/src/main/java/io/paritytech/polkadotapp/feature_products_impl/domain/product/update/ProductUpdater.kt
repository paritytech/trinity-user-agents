package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ProductUpdater @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val productsInUse: ProductsInUse,
    private val dotNsTldProvider: DotNsTldProvider,
    private val resolveProductUseCase: ResolveProductUseCase,
) {
    private val checkLocks = KeyedMutex<String>()

    fun observePending(): Flow<Set<ProductId>> {
        return observePendingNames()
            .map { names -> names.mapNotNull { productIdOf(it) }.toSet() }
            .distinctUntilChanged()
    }

    fun observePendingNames(): Flow<Set<String>> = dotNsResolver.observePendingUpdates()

    suspend fun check(productId: ProductId): Result<Unit> = fetchUpdates(mapOf(productId.value to namesOf(productId)))

    suspend fun checkAll(): Result<Unit> {
        val names = dotNsResolver.namesOnDevice()
        return fetchUpdates(names.groupBy { productIdOf(it)?.value ?: it })
    }

    suspend fun apply(productId: ProductId) = applyNames(productId, namesOf(productId))

    suspend fun applyPage(productId: ProductId, pageName: String) = applyNames(productId, listOf(pageName))

    fun start(scope: CoroutineScope) {
        productsInUse.observe()
            .scan(emptySet<ProductId>() to emptySet<ProductId>()) { (previous, _), current -> current to (current - previous) }
            .onEach { (_, started) ->
                started.forEach { productId ->
                    scope.launch { check(productId).logFailure("Failed to check $productId for an update") }
                }
            }
            .launchIn(scope)

        combine(observePending(), productsInUse.observe()) { pending, inUse -> pending - inUse }
            .onEach { idle -> idle.forEach { apply(it) } }
            .launchIn(scope)
    }

    private suspend fun applyNames(productId: ProductId, names: List<String>) {
        val pending = observePendingNames().first()
        val pendingNames = names.filter { it in pending }
        if (pendingNames.isEmpty()) return

        dotNsResolver.applyUpdates(pendingNames)
        resolveProductUseCase.invalidate(productId)
    }

    private suspend fun fetchUpdates(namesByProduct: Map<String, List<String>>): Result<Unit> {
        val results = namesByProduct.entries.mapAsync { (product, names) ->
            checkLocks.withLock(product) { dotNsResolver.fetchUpdates(names) }
        }
        val failure = results.firstNotNullOfOrNull { it.exceptionOrNull() }

        return if (failure != null && results.all { it.isFailure }) Result.failure(failure) else Result.success(Unit)
    }

    private suspend fun namesOf(productId: ProductId): List<String> {
        return dotNsResolver.namesOnDevice().filter { productIdOf(it) == productId }
    }

    private suspend fun productIdOf(dotNsName: String): ProductId? {
        return dotNsTldProvider.getTld().getOrNull()?.let { tld -> productIdFromHost(dotNsName, tld).getOrNull() }
    }
}
