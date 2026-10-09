package io.paritytech.polkadotapp.feature_products_api.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/** A funding session waiting on the overlay. [productId] is `null` when the host opened it. */
data class FundingOverlayRequest(
    val productId: ProductId?,
    val intent: String,
    val direction: FundingDirection,
    val amount: Balance?,
)

enum class FundingOverlayOutcome {
    STARTED,
    DISMISSED,
}

enum class ProviderFrameOutcome {
    CLOSED,
    DISMISSED,
}

/** The host UI the core's funding sessions are shown in. */
interface FundingOverlay {
    /**
     * Shows the overlay for [request] and answers once the user started or dismissed it. The session stays open
     * while this suspends, so the provider has to be chosen before answering [FundingOverlayOutcome.STARTED].
     */
    suspend fun present(request: FundingOverlayRequest): FundingOverlayOutcome

    /** Shows provider [providerId]'s screen at [route] for session [intent] and answers how it closed. */
    suspend fun presentProviderFrame(providerId: ProductId, intent: String, route: String): ProviderFrameOutcome
}
