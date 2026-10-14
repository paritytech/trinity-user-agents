package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.database.dao.ProductDao
import io.paritytech.polkadotapp.database.model.ProductLocal
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

interface ProductRepository {
    fun observeProducts(): Flow<List<Product>>

    suspend fun getProductById(id: ProductId): Product?

    /** Debug-menu worker URL for [id], or null if none. */
    suspend fun getUserWorkerUrl(id: ProductId): String?

    suspend fun addProduct(id: ProductId, name: String): ProductId

    /** Separate from [upsertResolvedProduct], which must not clobber a set `userWorkerUrl`. */
    suspend fun upsertManualProduct(id: ProductId, name: String, userWorkerUrl: String)

    /** Writes name + icon without clobbering integrations, permissions or `userWorkerUrl`. */
    suspend fun upsertResolvedProduct(product: Product)

    suspend fun deleteProduct(id: ProductId)
}

@Singleton
class RealProductRepository @Inject constructor(
    private val productDao: ProductDao,
) : ProductRepository {
    override fun observeProducts(): Flow<List<Product>> {
        return productDao.observeAll()
            .map { products -> products.map { it.toProduct() } }
    }

    override suspend fun getProductById(id: ProductId): Product? {
        return productDao.getById(id.value)?.toProduct()
    }

    override suspend fun getUserWorkerUrl(id: ProductId): String? {
        return productDao.getUserWorkerUrl(id.value)
    }

    override suspend fun addProduct(id: ProductId, name: String): ProductId {
        productDao.insert(
            ProductLocal(
                id = id.value,
                name = name,
                iconCid = null,
                iconFormat = null,
                userWorkerUrl = null,
            )
        )
        return id
    }

    override suspend fun upsertManualProduct(id: ProductId, name: String, userWorkerUrl: String) {
        productDao.upsertManual(
            ProductLocal(
                id = id.value,
                name = name,
                iconCid = null,
                iconFormat = null,
                userWorkerUrl = userWorkerUrl,
            )
        )
    }

    override suspend fun upsertResolvedProduct(product: Product) {
        productDao.upsertResolved(product.toLocal())
    }

    override suspend fun deleteProduct(id: ProductId) {
        productDao.deleteById(id.value)
    }
}
