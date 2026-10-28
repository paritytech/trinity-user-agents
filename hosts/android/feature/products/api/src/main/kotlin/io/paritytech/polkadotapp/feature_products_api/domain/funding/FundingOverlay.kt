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

/** What a provider asks the host to show in its frame. */
sealed interface ProviderFrameTarget {
    /** A route of the provider's own App. */
    data class App(val route: String) : ProviderFrameTarget

    /** An external https page, such as an onramp's widget, finished once it reaches [returnUrl]. */
    data class Page(val url: String, val returnUrl: String?) : ProviderFrameTarget
}

sealed interface ProviderFrameOutcome {
    /** The screen finished and closed itself. */
    data object Closed : ProviderFrameOutcome

    /** The user closed it. */
    data object Dismissed : ProviderFrameOutcome

    /** The page reached its return URL; [url] is the full URL, query included. */
    data class Returned(val url: String) : ProviderFrameOutcome
}

/** The host UI the core's funding sessions are shown in. */
interface FundingOverlay {
    /**
     * Shows the overlay for [request] and answers once the user started or dismissed it. The session stays open
     * while this suspends, so the provider has to be chosen before answering [FundingOverlayOutcome.STARTED].
     */
    suspend fun present(request: FundingOverlayRequest): FundingOverlayOutcome

    /** Shows [target] for provider [providerId]'s session [intent] and answers how it closed. */
    suspend fun presentProviderFrame(providerId: ProductId, intent: String, target: ProviderFrameTarget): ProviderFrameOutcome
}
