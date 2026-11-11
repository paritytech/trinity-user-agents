package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CompletableDeferred

/** How the funding sheet was opened. */
sealed interface FundingOverlayMode {
    /** For a session the core has just opened and is waiting on. */
    data object New : FundingOverlayMode

    /** For a session the core still holds, opened again from the CASH card. Nothing waits on its answer. */
    data object Resumed : FundingOverlayMode
}

/** One overlay the core is waiting on. The first answer wins; later ones are the overlay closing after it. */
class FundingOverlayContext(
    val request: FundingOverlayRequest,
    val mode: FundingOverlayMode,
) {
    private val outcome = CompletableDeferred<FundingOverlayOutcome>()
    private val shown = CompletableDeferred<Unit>()

    /** Set once the sheet should be gone; a sheet that could not be closed under another screen closes on its return. */
    @Volatile
    var isClosed: Boolean = false
        private set

    /** Called by the sheet once it holds this context. */
    fun markShown() {
        shown.complete(Unit)
    }

    fun answer(answer: FundingOverlayOutcome) {
        outcome.complete(answer)
        shown.complete(Unit)
    }

    fun close() {
        isClosed = true
    }

    suspend fun awaitOutcome(): FundingOverlayOutcome = outcome.await()

    suspend fun awaitShownOrAnswered() = shown.await()
}

/** The overlays on screen, keyed by session, so a sheet finds the one it was opened for. */
@Singleton
class FundingOverlayContexts @Inject constructor() {
    private val contexts = ConcurrentHashMap<String, FundingOverlayContext>()

    fun put(context: FundingOverlayContext) {
        contexts[context.request.intent] = context
    }

    fun get(intent: String): FundingOverlayContext? = contexts[intent]

    /** Removes [context] only while it is still the one on screen for its session. */
    fun remove(context: FundingOverlayContext) {
        contexts.remove(context.request.intent, context)
    }
}
