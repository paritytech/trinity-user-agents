package io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions

import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.NativeMediaPermissions
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.NativeMediaPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class ProductPermissionsInteractor @Inject constructor(
    private val productRepository: ProductRepository,
    private val permissionRepository: ProductPermissionRepository,
    private val mediaPermissions: NativeMediaPermissions,
) {
    suspend fun getProduct(productId: ProductId): Product? {
        return productRepository.getProductById(productId)
    }

    fun observePermissions(productId: ProductId): Flow<List<ProductPermissionStatus>> {
        return permissionRepository.observeAllByProduct(productId).map { permissions ->
            permissions.filterNot {
                when ((it.permission as? ProductPermission.DeviceCapability)?.capability) {
                    DeviceCapabilityType.Camera, DeviceCapabilityType.Microphone -> true
                    else -> false
                }
            }
        }
    }

    fun observeMediaPermissions(productId: ProductId) = mediaPermissions.observeMediaPermissions(productId)

    suspend fun toggleMediaPermission(productId: ProductId, permission: NativeMediaPermissionStatus) =
        mediaPermissions.toggleMediaPermission(productId, permission)

    suspend fun togglePermission(productId: ProductId, permissionStatus: ProductPermissionStatus) {
        if (permissionStatus.granted.not()) {
            permissionRepository.grant(productId, permissionStatus.permission)
        } else {
            permissionRepository.revoke(productId, permissionStatus.permission)
        }
    }
}
