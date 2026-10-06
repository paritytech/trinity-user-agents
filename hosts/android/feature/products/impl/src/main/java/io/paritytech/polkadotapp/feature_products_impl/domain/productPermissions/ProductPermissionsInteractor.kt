package io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions

import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class ProductPermissionsInteractor @Inject constructor(
    private val productRepository: ProductRepository,
    private val permissionSettings: ProductPermissionSettings
) {
    suspend fun getProduct(productId: ProductId): Product? {
        return productRepository.getProductById(productId)
    }

    fun observePermissions(productId: ProductId): Flow<List<SavedProductPermission>> {
        return permissionSettings.observePermissions(productId)
    }

    suspend fun togglePermission(productId: ProductId, permissionStatus: SavedProductPermission) {
        permissionSettings.togglePermission(productId, permissionStatus)
    }
}
