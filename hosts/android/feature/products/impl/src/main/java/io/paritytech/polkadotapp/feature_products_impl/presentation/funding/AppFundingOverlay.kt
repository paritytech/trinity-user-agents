package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.session
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toDomain
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import javax.inject.Inject
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.ProductId

/**
 * Shows each session the core opens in the funding sheet, and answers once the user started or left it. A
 * provider's own screen opens over the sheet.
 *
 * A started session runs on without its screens, so leaving one closes the screen but not the session. A frame left
 * that way is not answered, because a provider reads an early answer as the user giving up; the CASH card opens it
 * again, loading the provider's page afresh. A frame answers `Closed` when the user says they sent a bank transfer.
 * An external page answers `Returned` once it reaches its return URL.
 *
 * A frame closes, answering `Closed`, once the provider's part is done: the core closes it once the payment moved,
 * and a provider may close its own, such as after a card payment the user made. The session's sheet shows in its
 * place.
 *
 * The core drops a call when its product or worker goes away. The sheet or frame it showed then closes, rather than
 * staying up with nothing to answer. One that cannot close because another screen sits on it closes itself once it is
 * back on top.
 *
 * Navigation can fail silently. A sheet that does not appear in time is taken as dismissed, so the core does not wait
 * on it forever and the CASH card can open the session again.
 */
class AppFundingOverlay @Inject constructor(
    private val contexts: FundingOverlayContexts,
    private val frames: FundingFrameContexts,
    private val runtime: FundingRuntime,
    private val router: ProductsRouter,
) : FundingOverlay {
    override suspend fun present(request: FundingOverlayRequest): FundingOverlayOutcome {
        val context = FundingOverlayContext(request, FundingOverlayMode.New)
        contexts.put(context)
        var answered = false
        try {
            router.openFundingOverlay(request.intent)
            if (!context.awaitShown()) return FundingOverlayOutcome.DISMISSED
            return context.awaitOutcome().also { answered = true }
        } finally {
            // An answered sheet closes itself; one the core stopped waiting on is closed here.
            if (!answered) {
                contexts.remove(context)
                context.close()
                withContext(NonCancellable) { router.closeFundingOverlay(request.intent) }
            }
        }
    }

    override suspend fun presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: ProviderFrameTarget,
    ): ProviderFrameOutcome {
        val frame = FundingFrameContext(providerId = providerId, intent = intent, target = target)
        frames.put(frame)
        val outcome = try {
            router.openFundingProviderFrame(intent)
            frame.awaitOutcome()
        } finally {
            frames.remove(frame)
            withContext(NonCancellable) { router.closeFundingProviderFrame(intent) }
        }
        if (frame.awaitClosedByProvider() && !reopen(intent)) router.openFundingEnded(intent)
        return outcome
    }

    override fun closeProviderFrame(providerId: ProductId, intent: String) {
        frames.get(intent)?.takeIf { it.providerId == providerId }?.closeByProvider()
    }

    /**
     * Shows a session in flight again, on the screen it stands at, with the provider's screen over it while the
     * provider still waits on one. Answers `false` for a session the core says is no longer open, ended even while it
     * still pushes it, which shows on its outcome instead.
     */
    suspend fun reopen(intent: String): Boolean {
        if (!runtime.sessionOpen(intent)) return false
        val session = runtime.session(intent) ?: return false
        if (contexts.get(intent) != null) return true

        val request = FundingOverlayRequest(
            productId = null,
            intent = intent,
            direction = session.direction.toDomain(),
            amount = session.amount?.toBalance(),
        )
        val context = FundingOverlayContext(request, FundingOverlayMode.Resumed)
        contexts.put(context)
        router.openFundingOverlay(intent)
        if (!context.awaitShown()) {
            contexts.remove(context)
            return true
        }

        val frame = frames.get(intent)
        if (frame != null && !frame.isShown) router.openFundingProviderFrame(intent)
        return true
    }
}

private suspend fun FundingOverlayContext.awaitShown(): Boolean =
    withTimeoutOrNull(SHEET_SHOWN_TIMEOUT_MS) { awaitShownOrAnswered() } != null

private const val SHEET_SHOWN_TIMEOUT_MS = 10_000L
