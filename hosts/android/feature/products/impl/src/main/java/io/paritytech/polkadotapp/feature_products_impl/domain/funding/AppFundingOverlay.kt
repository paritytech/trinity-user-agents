package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import javax.inject.Inject

/** Shows each session the core opens in the funding sheet, and answers once the user started or left it. */
class AppFundingOverlay @Inject constructor(
    private val contexts: FundingOverlayContexts,
    private val router: ProductsRouter,
) : FundingOverlay {
    override suspend fun present(request: FundingOverlayRequest): FundingOverlayOutcome {
        val context = FundingOverlayContext(request)
        contexts.put(context)
        router.openFundingOverlay(request.intent)
        return context.awaitOutcome()
    }

    override suspend fun presentProviderFrame(providerId: ProductId, intent: String, route: String): ProviderFrameOutcome =
        ProviderFrameOutcome.DISMISSED
}
