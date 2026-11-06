package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.product.gatewayUrlOf
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import uniffi.truapi.ProductId
import javax.inject.Inject

/** How a provider is shown: its product's display name and icon. */
data class FundingProviderBrand(
    val providerId: ProductId,
    val name: String,
    val iconUrl: String?,
)

/**
 * Reads a provider's brand from its product's root manifest; the bare id stands in until it resolves. A
 * provider the app ships has no manifest read for it.
 */
class FundingProviderBranding @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val ipfsContentLookup: IpfsContentLookup,
    private val bundledProducts: BundledProducts,
) {
    suspend fun brand(providerId: ProductId): FundingProviderBrand {
        if (bundledProducts.product(providerId) != null) return placeholder(providerId)

        val product = resolveProductUseCase.resolve(providerId).getOrNull()?.product ?: return placeholder(providerId)

        return FundingProviderBrand(
            providerId = providerId,
            name = product.name,
            iconUrl = product.icon?.let { ipfsContentLookup.gatewayUrlOf(it) },
        )
    }

    fun placeholder(providerId: ProductId) = FundingProviderBrand(providerId = providerId, name = providerId.value, iconUrl = null)
}
