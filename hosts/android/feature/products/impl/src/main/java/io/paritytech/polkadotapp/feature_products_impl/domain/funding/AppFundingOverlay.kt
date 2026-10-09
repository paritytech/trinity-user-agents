package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingRail
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

/**
 * Shows each session the core opens in the funding sheet, and answers once the user started or left it. A
 * provider's own screen opens over the sheet, except for a card, whose sheet closes as the session starts.
 */
class AppFundingOverlay @Inject constructor(
    private val contexts: FundingOverlayContexts,
    private val frames: FundingFrameContexts,
    private val runtime: FundingRuntime,
    private val router: ProductsRouter,
) : FundingOverlay {
    private companion object {
        val SHEET_CLOSE_TIMEOUT = 2.seconds
    }

    override suspend fun present(request: FundingOverlayRequest): FundingOverlayOutcome {
        val context = FundingOverlayContext(request)
        contexts.put(context)
        router.openFundingOverlay(request.intent)
        return context.awaitOutcome()
    }

    override suspend fun presentProviderFrame(providerId: ProductId, intent: String, route: String): ProviderFrameOutcome {
        val session = runtime.session(intent)
        val rail = session?.choice?.rail ?: FundingRail.CARD

        if (rail == FundingRail.CARD) {
            contexts.get(intent)?.let { withTimeoutOrNull(SHEET_CLOSE_TIMEOUT) { it.awaitClosed() } }
        }

        val frame = FundingFrameContext(
            providerId = providerId,
            intent = intent,
            route = route,
            showsSentFunds = rail == FundingRail.BANK && session?.direction == FundingDirection.IN,
        )
        frames.put(frame)
        router.openFundingProviderFrame(intent)
        return frame.awaitOutcome().also { frames.remove(intent) }
    }
}
