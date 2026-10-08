package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import javax.inject.Inject

class ProductIconUrlUseCase @Inject constructor(
    private val productRepository: ProductRepository,
    private val ipfsContentLookup: IpfsContentLookup,
) {
    suspend operator fun invoke(productId: ProductId): String? {
        return productRepository.getProductById(productId)?.icon?.let { ipfsContentLookup.gatewayUrlOf(it) }
    }
}
