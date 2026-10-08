package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

/** Stands in until the funding overlay UI lands: every session reads as dismissed, so the core discards it. */
class DismissingFundingOverlay @Inject constructor() : FundingOverlay {
    override suspend fun present(request: FundingOverlayRequest): FundingOverlayOutcome = FundingOverlayOutcome.DISMISSED

    override suspend fun presentProviderFrame(providerId: ProductId, intent: String, route: String): ProviderFrameOutcome =
        ProviderFrameOutcome.DISMISSED
}
