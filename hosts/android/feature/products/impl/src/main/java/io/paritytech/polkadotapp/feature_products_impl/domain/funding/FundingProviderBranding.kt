package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.gatewayUrlOf
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import javax.inject.Inject

/** How a provider is shown: its product's display name and icon. */
data class FundingProviderBrand(
    val providerId: String,
    val name: String,
    val iconUrl: String?,
)

/** Reads a provider's brand from its product's root manifest; the bare id stands in until it resolves. */
class FundingProviderBranding @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val ipfsContentLookup: IpfsContentLookup,
) {
    suspend fun brand(providerId: String): FundingProviderBrand {
        val product = resolveProductUseCase.resolve(ProductId.fromStoredValue(providerId)).getOrNull()?.product
            ?: return placeholder(providerId)

        return FundingProviderBrand(
            providerId = providerId,
            name = product.name,
            iconUrl = product.icon?.let { ipfsContentLookup.gatewayUrlOf(it) },
        )
    }

    fun placeholder(providerId: String) = FundingProviderBrand(providerId = providerId, name = providerId, iconUrl = null)
}
