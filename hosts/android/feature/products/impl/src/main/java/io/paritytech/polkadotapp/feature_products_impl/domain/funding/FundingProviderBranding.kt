package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.product.gatewayUrlOf
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import uniffi.truapi.BundleEntry
import uniffi.truapi.ProductId
import javax.inject.Inject

/** How a provider is shown: its product's display name and icon. */
data class FundingProviderBrand(
    val providerId: ProductId,
    val name: String,
    val iconUrl: String?,
)

/**
 * Reads a provider's brand from its product's root manifest, the one the app ships for a bundled provider; the bare
 * id stands in until it resolves.
 */
class FundingProviderBranding @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val ipfsContentLookup: IpfsContentLookup,
    private val bundledProducts: BundledProducts,
) {
    suspend fun brand(providerId: ProductId): FundingProviderBrand {
        bundledProducts.entry(providerId)?.let { return brand(it) }

        val product = resolveProductUseCase.resolve(providerId).getOrNull()?.product ?: return placeholder(providerId)

        return FundingProviderBrand(
            providerId = providerId,
            name = product.name,
            iconUrl = product.icon?.let { ipfsContentLookup.gatewayUrlOf(it) },
        )
    }

    private suspend fun brand(entry: BundleEntry) = FundingProviderBrand(
        providerId = entry.productId,
        name = entry.displayName ?: entry.productId.value,
        iconUrl = entry.iconCid?.let { ipfsContentLookup.getIpfsLinkFor(it).logFailure("Bundled provider icon unresolved").getOrNull() },
    )

    fun placeholder(providerId: ProductId) = FundingProviderBrand(providerId = providerId, name = providerId.value, iconUrl = null)
}
