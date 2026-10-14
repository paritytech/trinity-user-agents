package io.paritytech.polkadotapp.feature_products_api.domain

import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Where the funding products the app launches from its own UI are served, unlike the
 * governance-reserved ones in [io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds].
 *
 * Configured remotely, so it can be repointed without a release.
 */
interface FundingDomainProvider {
    suspend fun getFundingConfig(): Result<FundingConfig>

    /** Products behind [FundingConfig.onrampUrl] and [FundingConfig.offrampUrl], resolved against the active network TLD. */
    suspend fun getFundingProductIds(): Result<Set<ProductId>>
}
