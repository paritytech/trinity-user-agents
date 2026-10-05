package io.paritytech.polkadotapp.feature_products_impl.data.repository

import dagger.Lazy
import io.paritytech.polkadotapp.database.dao.ProductDao
import io.paritytech.polkadotapp.database.model.ProductLocal
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.ProductRecord
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
    private val runtimeSettings: ProductRuntimeSettings,
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
) : ProductRepository {
    private suspend fun runtime() = runtimeProvider.get().runtime().getOrThrow()
    private suspend fun records() = runtime().products()
    private suspend fun save(record: ProductRecord) {
        runtime().saveProduct(record)
    }

    override fun observeProducts(): Flow<List<Product>> {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return runtimeProvider.get().observeRecords(emptyList()) { runtime -> runtime.products().map { it.toLocal().toProduct() } }
        return productDao.observeAll()
            .map { products -> products.map { it.toProduct() } }
    }

    override suspend fun getProductById(id: ProductId): Product? {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return records().firstOrNull { it.productId == id.value }?.toLocal()?.toProduct()
        return productDao.getById(id.value)?.toProduct()
    }

    override suspend fun getUserWorkerUrl(id: ProductId): String? {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return records().firstOrNull { it.productId == id.value }?.workerUrlOverride
        return productDao.getUserWorkerUrl(id.value)
    }

    override suspend fun addProduct(id: ProductId, name: String): ProductId {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            save(ProductRecord(id.value, name, null, null, null))
            return id
        }
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
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            val existing = records().firstOrNull { it.productId == id.value }
            save(ProductRecord(id.value, name, existing?.iconCid, existing?.iconFormat, userWorkerUrl))
            return
        }
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
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            val existing = records().firstOrNull { it.productId == product.id.value }
            save(ProductRecord(product.id.value, product.name, product.icon?.cid?.toString(), product.icon?.format?.name, existing?.workerUrlOverride))
            return
        }
        productDao.upsertResolved(product.toLocal())
    }

    override suspend fun deleteProduct(id: ProductId) {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            runtime().removeProduct(id.value)
            return
        }
        productDao.deleteById(id.value)
    }
}

private fun ProductRecord.toLocal() = ProductLocal(productId, name, iconCid, iconFormat, workerUrlOverride)
