package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import kotlinx.coroutines.CompletableDeferred
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

/** A provider's own screen the core is waiting on. The first answer wins. */
class FundingFrameContext(
    val providerId: ProductId,
    val intent: String,
    val route: String,
    val showsSentFunds: Boolean,
) {
    private val outcome = CompletableDeferred<ProviderFrameOutcome>()

    val url: String
        get() {
            val base = providerId.toUrl().trimEnd('/')
            return if (route.startsWith("/") || route.startsWith("#")) base + route else "$base/$route"
        }

    fun answer(answer: ProviderFrameOutcome) {
        outcome.complete(answer)
    }

    suspend fun awaitOutcome(): ProviderFrameOutcome = outcome.await()
}

@Singleton
class FundingFrameContexts @Inject constructor() {
    private val contexts = ConcurrentHashMap<String, FundingFrameContext>()

    fun put(context: FundingFrameContext) {
        contexts[context.intent] = context
    }

    fun get(intent: String): FundingFrameContext? = contexts[intent]

    fun remove(intent: String) {
        contexts.remove(intent)
    }
}
