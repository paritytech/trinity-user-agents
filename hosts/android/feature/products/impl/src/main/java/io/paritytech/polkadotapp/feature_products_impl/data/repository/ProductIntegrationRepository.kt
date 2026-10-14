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
) : ProductIntegrationRepository {
    override fun observeByProduct(productId: ProductId): Flow<List<ProductIntegration>> {
        return dao.observeByProduct(productId.value).map { list -> list.map { it.toDomain() } }
    }

    override fun observeByType(type: IntegrationType): Flow<List<ProductIntegration>> {
        return dao.observeByType(type.toLocal()).map { list -> list.map { it.toDomain() } }
    }

    override fun observeProductsByType(type: IntegrationType): Flow<List<Product>> {
        return dao.observeProductsByIntegrationType(type.toLocal())
            .map { list -> list.map { it.toProduct() } }
    }

    override suspend fun get(productId: ProductId, type: IntegrationType): ProductIntegration? {
        return dao.get(productId.value, type.toLocal())?.toDomain()
    }

    override suspend fun install(productId: ProductId, type: IntegrationType) {
        dao.insert(
            ProductIntegrationLocal(
                productId = productId.value,
                type = type.toLocal(),
                metadata = null
            )
        )
    }

    override suspend fun uninstall(productId: ProductId, type: IntegrationType) {
        dao.delete(productId.value, type.toLocal())
    }

    override suspend fun uninstallAll(productId: ProductId) {
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
