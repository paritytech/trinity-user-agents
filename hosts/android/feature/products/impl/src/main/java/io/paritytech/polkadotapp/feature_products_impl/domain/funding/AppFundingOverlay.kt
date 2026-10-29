package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toDomain
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStep
import javax.inject.Inject

/**
 * Shows each session the core opens in the funding sheet, and answers once the user started or left it. A
 * provider's own screen opens over the sheet.
 *
 * A started session runs on without its screens, so leaving one only hides it. A hidden frame is not answered,
 * because a provider reads an early answer as the user giving up; the CASH card can show the frame again. A frame
 * answers `Closed` when the user says they sent a bank transfer, or once the provider's part is done: the payment
 * seen or the session over. An external page answers `Returned` once it reaches its return URL.
 *
 * The core drops a call when its product or worker goes away. The sheet or frame it showed then closes, rather than
 * staying up with nothing to answer.
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
            return context.awaitOutcome().also { answered = true }
        } finally {
            // An answered sheet closes itself; one the core stopped waiting on is closed here.
            if (!answered) {
                contexts.remove(context)
                withContext(NonCancellable) { router.closeFundingOverlay(request.intent) }
            }
        }
    }

    override suspend fun presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: ProviderFrameTarget,
    ): ProviderFrameOutcome {
        val session = runtime.session(intent)
        val rail = session?.choice?.rail ?: FundingRail.CARD

        val frame = FundingFrameContext(
            providerId = providerId,
            intent = intent,
            target = target,
            showsSentFunds = rail == FundingRail.BANK && session?.direction == FundingDirection.IN,
        )
        frames.put(frame)
        try {
            router.openFundingProviderFrame(intent)

            return coroutineScope {
                // The frame may be hidden when the payment moves, so the overlay, not the screen, watches for it.
                val watcher = launch {
                    runtime.sessionChanges()
                        .filter { it == intent }
                        .map { }
                        .onStart { emit(Unit) }
                        .first { runtime.paymentMoved(intent) }
                    frame.answer(ProviderFrameOutcome.Closed)
                }
                frame.awaitOutcome().also { watcher.cancel() }
            }
        } finally {
            frames.remove(frame)
            withContext(NonCancellable) { router.closeFundingProviderFrame(intent) }
        }
    }

    /**
     * Shows a session the core still holds again: in flight on the screen it stands at, with the provider's
     * screen over it while the provider still waits on one, or ended on its outcome.
     */
    suspend fun reopen(intent: String): Boolean {
        if (contexts.get(intent) != null) return true
        val session = runtime.session(intent) ?: return false

        val request = FundingOverlayRequest(
            productId = null,
            intent = intent,
            direction = session.direction.toDomain(),
            amount = session.amount?.toBalance(),
        )
        contexts.put(FundingOverlayContext(request, FundingOverlayMode.Resumed))
        router.openFundingOverlay(intent)

        val frame = frames.get(intent)
        if (session.stage.isOpen && frame != null && !frame.isShown) router.openFundingProviderFrame(intent)
        return true
    }
}

/** The provider's screen has done its part once the payment is seen or the session is over. */
suspend fun FundingRuntime.paymentMoved(intent: String): Boolean {
    val session = session(intent) ?: return true
    if (!session.stage.isOpen) return true

    return progress(intent)?.steps.orEmpty().any { it.reachedAtMs != null && it.step != FundingStep.STARTED }
}
