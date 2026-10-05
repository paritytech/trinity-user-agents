package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.database.dao.ProductIntegrationDao
import io.paritytech.polkadotapp.database.model.ProductIntegrationLocal
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.IntegrationType
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductIntegration
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

interface ProductIntegrationRepository {
    fun observeByProduct(productId: ProductId): Flow<List<ProductIntegration>>

    fun observeByType(type: IntegrationType): Flow<List<ProductIntegration>>

    fun observeProductsByType(type: IntegrationType): Flow<List<Product>>

    suspend fun get(productId: ProductId, type: IntegrationType): ProductIntegration?

    suspend fun install(productId: ProductId, type: IntegrationType)

    suspend fun uninstall(productId: ProductId, type: IntegrationType)

    suspend fun uninstallAll(productId: ProductId)
}

@Singleton
class RealProductIntegrationRepository @Inject constructor(
    private val dao: ProductIntegrationDao,
    private val runtimeSettings: io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings,
    private val runtimeProvider: dagger.Lazy<io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider>,
    private val products: ProductRepository,
) : ProductIntegrationRepository {
    private suspend fun runtime() = runtimeProvider.get().runtime().getOrThrow()
    private suspend fun installed(): List<ProductIntegration> = runtime().workerProducts().mapNotNull { worker ->
        if (runtime().workerReasons(worker.productId).contains(uniffi.truapi.WorkerReason.Chat)) {
            ProductIntegration(ProductId.fromStoredValue(worker.productId), IntegrationType.Chat)
        } else null
    }

    override fun observeByProduct(productId: ProductId): Flow<List<ProductIntegration>> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtimeProvider.get().observeRecords(emptyList()) { installed().filter { it.productId == productId } }
        return dao.observeByProduct(productId.value).map { list -> list.map { it.toDomain() } }
    }

    override fun observeByType(type: IntegrationType): Flow<List<ProductIntegration>> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtimeProvider.get().observeRecords(emptyList()) { installed().filter { it.type == type } }
        return dao.observeByType(type.toLocal()).map { list -> list.map { it.toDomain() } }
    }

    override fun observeProductsByType(type: IntegrationType): Flow<List<Product>> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return observeByType(type).map { integrations -> integrations.mapNotNull { products.getProductById(it.productId) } }
        return dao.observeProductsByIntegrationType(type.toLocal())
            .map { list -> list.map { it.toProduct() } }
    }

    override suspend fun get(productId: ProductId, type: IntegrationType): ProductIntegration? {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return installed().firstOrNull { it.productId == productId && it.type == type }
        return dao.get(productId.value, type.toLocal())?.toDomain()
    }

    override suspend fun install(productId: ProductId, type: IntegrationType) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            runtime().notifyWorkerIntent(productId.value, uniffi.truapi.WorkerIntentAction.ADD, uniffi.truapi.WorkerModality.Chat)
            runtimeProvider.get().notifyRecordsChanged()
            return
        }
        dao.insert(
            ProductIntegrationLocal(
                productId = productId.value,
                type = type.toLocal(),
                metadata = null
            )
        )
    }

    override suspend fun uninstall(productId: ProductId, type: IntegrationType) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            runtime().notifyWorkerIntent(productId.value, uniffi.truapi.WorkerIntentAction.REMOVE, uniffi.truapi.WorkerModality.Chat)
            runtimeProvider.get().notifyRecordsChanged()
            return
        }
        dao.delete(productId.value, type.toLocal())
    }

    override suspend fun uninstallAll(productId: ProductId) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return uninstall(productId, IntegrationType.Chat)
        dao.deleteAllByProduct(productId.value)
    }

    private fun IntegrationType.toLocal(): String = when (this) {
        is IntegrationType.Chat -> "CHAT"
    }

    private fun ProductIntegrationLocal.toDomain(): ProductIntegration {
        return ProductIntegration(
            productId = ProductId.fromStoredValue(productId),
            type = typeFromLocal(type)
        )
    }

    private fun typeFromLocal(type: String): IntegrationType = when (type) {
        "CHAT" -> IntegrationType.Chat
        else -> error("Unknown integration type: $type")
    }
}
