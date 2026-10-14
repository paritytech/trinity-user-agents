package io.paritytech.polkadotapp.feature_products_impl.domain.productSettings

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.gatewayUrlOf
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class ProductSettingsInteractor @Inject constructor(
    private val productRepository: ProductRepository,
    private val permissionRepository: ProductPermissionRepository,
    private val ipfsContentLookup: IpfsContentLookup,
) {
    fun observeProductSettings(productId: ProductId): Flow<ProductSettingsInfo?> {
        return combine(
            productRepository.observeProducts().map { products -> products.find { it.id == productId } },
            permissionRepository.observeHasAnyPermissionRequested(productId),
        ) { product, hasPermissions ->
            product?.let { ProductSettingsInfo(it, hasPermissions, it.icon?.let { icon -> ipfsContentLookup.gatewayUrlOf(icon) }) }
        }
    }
}
