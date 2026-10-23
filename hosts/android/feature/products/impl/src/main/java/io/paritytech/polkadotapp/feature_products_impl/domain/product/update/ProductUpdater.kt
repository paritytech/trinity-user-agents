package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Keeps the product builds on the device current. A product coming into use is checked for a newer build, which
 * downloads beside the one running. The download is applied once nothing of the product is on screen, or when the
 * user agrees to reload the product's page, so no page or worker switches builds under them.
 */
@Singleton
class ProductUpdater @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val productsInUse: ProductsInUse,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    /** Products with a downloaded build waiting to be applied. */
    fun observePending(): Flow<Set<ProductId>> {
        return observePendingNames()
            .map { names -> names.mapNotNull { productIdOf(it) }.toSet() }
            .distinctUntilChanged()
    }

    /** Names with a downloaded build waiting to be applied. */
    fun observePendingNames(): Flow<Set<String>> = dotNsResolver.observePendingUpdates()

    /** Downloads a newer build of each of [productId]'s names on the device, where one is published. */
    suspend fun check(productId: ProductId): Result<Unit> = fetchUpdates(namesOf(productId))

    /** [check] for every product on the device. */
    suspend fun checkAll(): Result<Unit> = fetchUpdates(dotNsResolver.namesOnDevice())

    /** Serves the downloaded builds of [productId] from its next load on. */
    suspend fun apply(productId: ProductId) {
        namesOf(productId).forEach { dotNsResolver.applyUpdate(it) }
    }

    /** Serves the downloaded build of [dotNsName] alone from its next request on. */
    suspend fun applyName(dotNsName: String) = dotNsResolver.applyUpdate(dotNsName)

    /** Runs the policy for the lifetime of [scope]. */
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

    // A name chain no longer answers for must not fail the run; a run that reached nothing did fail.
    private suspend fun fetchUpdates(dotNsNames: Collection<String>): Result<Unit> {
        val results = dotNsNames.mapAsync { dotNsResolver.fetchUpdate(it) }
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
