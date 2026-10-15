package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import kotlinx.coroutines.CompletableDeferred
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

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
    private val closed = CompletableDeferred<Unit>()

    fun answer(answer: FundingOverlayOutcome) {
        outcome.complete(answer)
    }

    suspend fun awaitOutcome(): FundingOverlayOutcome = outcome.await()

    fun markClosed() {
        closed.complete(Unit)
    }

    suspend fun awaitClosed() = closed.await()
}

/** The overlays on screen, keyed by session, so a sheet finds the one it was opened for. */
@Singleton
class FundingOverlayContexts @Inject constructor() {
    private val contexts = ConcurrentHashMap<String, FundingOverlayContext>()

    fun put(context: FundingOverlayContext) {
        contexts[context.request.intent] = context
    }

    fun get(intent: String): FundingOverlayContext? = contexts[intent]

    fun remove(intent: String) {
        contexts.remove(intent)
    }

    /** Removes [context] only while it is still the one on screen for its session. */
    fun remove(context: FundingOverlayContext) {
        contexts.remove(context.request.intent, context)
    }
}
