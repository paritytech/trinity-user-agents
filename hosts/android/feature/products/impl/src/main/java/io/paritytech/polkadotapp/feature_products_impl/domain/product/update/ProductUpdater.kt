package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
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
import javax.inject.Inject
import javax.inject.Singleton

// A product's version is its manifest records and the content of its names, checked and applied as one. It is applied
// once nothing of the product runs, or when the user reloads its page; a running worker keeps the code it loaded.
@Singleton
class ProductUpdater @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val productsInUse: ProductsInUse,
    private val dotNsTldProvider: DotNsTldProvider,
    private val resolveProductUseCase: ResolveProductUseCase,
) {
    // A screen start and the product coming into use check at the same moment; the second check waits for the first
    // and finds its names just read, so it does not read them again.
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

    suspend fun apply(productId: ProductId) {
        val pending = observePendingNames().first()
        val names = namesOf(productId).filter { it in pending }
        if (names.isEmpty()) return

        dotNsResolver.applyUpdates(names)
        // The manifest may have changed with the version, and the next resolve has to read the one now served.
        resolveProductUseCase.invalidate(productId)
    }

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

    // A product chain no longer answers for must not fail the run; a run that reached nothing did fail.
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
        return dotNsTldProvider.getTld().getOrNull()?.let { tld -> ProductId.fromString(dotNsName, tld).getOrNull() }
    }
}
