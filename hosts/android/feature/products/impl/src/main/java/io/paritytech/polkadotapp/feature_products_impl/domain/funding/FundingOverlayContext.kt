package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import kotlinx.coroutines.CompletableDeferred
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

/** One overlay the core is waiting on. The first answer wins; later ones are the overlay closing after it. */
class FundingOverlayContext(
    val request: FundingOverlayRequest,
) {
    private val outcome = CompletableDeferred<FundingOverlayOutcome>()

    fun answer(answer: FundingOverlayOutcome) {
        outcome.complete(answer)
    }

    suspend fun awaitOutcome(): FundingOverlayOutcome = outcome.await()
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
}
