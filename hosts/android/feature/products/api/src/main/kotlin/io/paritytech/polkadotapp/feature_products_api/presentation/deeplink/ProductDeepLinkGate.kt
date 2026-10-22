package io.paritytech.polkadotapp.feature_products_api.presentation.deeplink

import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Which products the host opens deeplinks for. Off the arbitrary-products flag only the app's own
 * funding products are reachable, so no link can put an unvetted product on screen or run its worker.
 *
 * Shared by every dotNS handler: a product the app refuses to browse must not become reachable
 * through another target either.
 */
class ProductDeepLinkGate(
    private val arbitraryProductsEnabled: Boolean,
    private val fundingDomainProvider: FundingDomainProvider,
) {
    suspend fun opens(productId: ProductId?): Boolean {
        if (arbitraryProductsEnabled) return true
        if (productId == null) return false

        return productId in fundingDomainProvider.getFundingProductIds().getOrNull().orEmpty()
    }
}
