package io.paritytech.polkadotapp.feature_products_impl.domain

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.product.gatewayUrlOf
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/** Identity plus an already-resolved icon URL. */
class ProductListEntry(
    val id: ProductId,
    val name: String,
    val iconUrl: String?,
)

class ProductListInteractor @Inject constructor(
    private val productRepository: ProductRepository,
    private val ipfsContentLookup: IpfsContentLookup,
) {
    fun observeProducts(): Flow<List<ProductListEntry>> {
        return productRepository.observeProducts().map { products ->
            products.map { product ->
                ProductListEntry(
                    id = product.id,
                    name = product.name,
                    iconUrl = product.icon?.let { ipfsContentLookup.gatewayUrlOf(it) },
                )
            }
        }
    }
}
