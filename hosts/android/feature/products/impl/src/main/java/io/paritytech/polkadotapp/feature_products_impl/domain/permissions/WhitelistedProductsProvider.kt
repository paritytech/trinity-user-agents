package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

interface WhitelistedProductsProvider {
    suspend fun whitelistedProducts(): Set<ProductId>
}

class RealWhitelistedProductsProvider @Inject constructor(
    private val fundingDomainProvider: FundingDomainProvider,
) : WhitelistedProductsProvider {
    // The funding products are whitelisted only while there is no product settings UI to grant them
    // permissions through.
    override suspend fun whitelistedProducts(): Set<ProductId> {
        if (FeatureOption.PRODUCT_SETTINGS.isEnabled) return emptySet()

        return fundingDomainProvider.getFundingProductIds()
            .logFailure("Failed to resolve the funding products to whitelist")
            .getOrDefault(emptySet())
    }
}
